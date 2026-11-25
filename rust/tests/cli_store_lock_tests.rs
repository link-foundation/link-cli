//! Concurrent `clink` invocations on one store (issue #194): every run that
//! exits 0 keeps its write, and a run that cannot get the store in time fails
//! with "store is locked" and the name of the owner.

use link_cli::store_lock::{lock_dir_path, owner_file_path, LockOwner};
use link_cli::{lock_file_path, FileLock, LockMode};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;

fn clink(db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_clink"))
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .unwrap()
}

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn failure(output: Output, message: &str) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains(message), "{stderr}");
    stderr
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The reproduction of the issue: N concurrent `--in` imports, then every
/// import that exited 0 must be in `--out`.
#[test]
fn concurrent_imports_that_succeed_are_all_durable() {
    for lock in ["flock", "pid"] {
        let dir = tempdir().unwrap();
        let db = dir.path().join("s.links");
        let writers: Vec<_> = (1..=12)
            .map(|i| {
                let input = dir.path().join(format!("in{i}.lino"));
                std::fs::write(&input, format!("(w{i}: w{i} w{i})\n")).unwrap();
                let db = db.clone();
                std::thread::spawn(move || {
                    let input = input.to_string_lossy().into_owned();
                    (i, clink(&db, &["--lock", lock, "--in", &input]))
                })
            })
            .collect();
        let succeeded: Vec<u32> = writers
            .into_iter()
            .map(|writer| writer.join().unwrap())
            .filter(|(_, output)| output.status.success())
            .map(|(i, _)| i)
            .collect();
        assert_eq!(succeeded.len(), 12, "{lock}: every writer gets the store");
        let out = dir.path().join("out.lino");
        success(clink(&db, &["--out", &out.to_string_lossy()]));
        let exported = std::fs::read_to_string(&out).unwrap();
        for i in succeeded {
            assert!(
                exported.contains(&format!("(w{i}: w{i} w{i})")),
                "{lock} lost w{i}: {exported}"
            );
        }
        // No temporary save, owner record or pid lock is left behind.
        assert!(
            entries(dir.path())
                .iter()
                .all(|name| !name.ends_with(".saving")
                    && !name.ends_with(".owner")
                    && !name.contains(".lock.d")),
            "{:?}",
            entries(dir.path())
        );
    }
}

fn hold_exclusive(db: &Path) -> FileLock {
    let lock = FileLock::acquire(lock_file_path(db), LockMode::Exclusive).unwrap();
    std::fs::write(
        owner_file_path(db),
        LockOwner::current("held-by-test").to_record(),
    )
    .unwrap();
    lock
}

#[test]
fn a_held_flock_times_out_naming_its_owner_and_no_lock_bypasses_it() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("s.links");
    success(clink(&db, &["() ((1 1))"]));
    let held = hold_exclusive(&db);
    let pid = format!("pid={}", std::process::id());

    let stderr = failure(
        clink(&db, &["--lock-timeout", "100", "() ((1 2))"]),
        "store is locked",
    );
    assert!(
        stderr.contains(&pid) && stderr.contains("100 ms"),
        "{stderr}"
    );
    // Readers wait for writers as well.
    failure(clink(&db, &["--lock-timeout=0", "--after"]), &pid);

    let status = success(clink(&db, &["--lock-status"]));
    assert!(
        status.contains("locked by an exclusive writer") && status.contains(&pid),
        "{status}"
    );

    // A caller that serializes access itself opts out.
    success(clink(&db, &["--no-lock", "() ((1 2))"]));
    success(clink(&db, &["--lock", "none", "() ((2 1))"]));
    drop(held);
    assert_eq!(
        success(clink(&db, &["--lock-timeout", "0", "--after"])),
        "(1: 1 1)\n(2: 1 2)\n(3: 2 1)\n"
    );
    // The reader cleared the record the crashed-looking holder left.
    assert!(!owner_file_path(&db).exists());
    let status = success(clink(&db, &["--lock-status"]));
    assert!(
        status.starts_with("flock ") && status.contains(": unlocked\npid "),
        "{status}"
    );
}

#[test]
fn readers_share_the_store_and_writers_wait_for_them() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("s.links");
    success(clink(&db, &["() ((1 1))"]));
    let reader = FileLock::acquire(lock_file_path(&db), LockMode::Shared).unwrap();
    let out = dir.path().join("out.lino");
    success(clink(
        &db,
        &["--lock-timeout", "0", "--out", &out.to_string_lossy()],
    ));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "(1: 1 1)\n");
    failure(
        clink(&db, &["--lock-timeout", "50", "() ((1 2))"]),
        "shared readers",
    );
    let status = success(clink(&db, &["--lock-status"]));
    assert!(status.contains("locked by shared readers"), "{status}");
    drop(reader);
    success(clink(&db, &["--lock-timeout", "0", "() ((1 2))"]));
}

fn write_pid_lock(db: &Path, pid: u32) -> PathBuf {
    let dir = lock_dir_path(db);
    std::fs::create_dir(&dir).unwrap();
    let owner = LockOwner {
        pid,
        ..LockOwner::current("planted-by-test")
    };
    std::fs::write(dir.join("owner"), owner.to_record()).unwrap();
    dir
}

#[test]
fn pid_locks_wait_for_live_owners_and_break_those_of_gone_ones() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("s.links");
    success(clink(&db, &["--lock", "pid", "() ((1 1))"]));
    assert!(!lock_dir_path(&db).exists());

    // This test process is alive: the lock holds.
    let lock_dir = write_pid_lock(&db, std::process::id());
    let pid = format!("pid={}", std::process::id());
    let stderr = failure(
        clink(
            &db,
            &["--lock", "pid", "--lock-timeout", "100", "() ((1 2))"],
        ),
        "store is locked",
    );
    assert!(
        stderr.contains(&pid) && stderr.contains(".lock.d"),
        "{stderr}"
    );
    let status = success(clink(&db, &["--lock-status"]));
    assert!(
        status.contains(&pid) && !status.contains("stale"),
        "{status}"
    );
    std::fs::remove_dir_all(&lock_dir).unwrap();

    // A process that has exited: the next writer breaks its lock.
    let mut child = Command::new(env!("CARGO_BIN_EXE_clink"))
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let gone = child.id();
    child.wait().unwrap();
    write_pid_lock(&db, gone);
    let status = success(clink(&db, &["--lock-status"]));
    assert!(
        status.contains("stale, the next writer breaks it"),
        "{status}"
    );
    let traced = clink(
        &db,
        &[
            "--lock",
            "pid",
            "--lock-timeout",
            "0",
            "--trace",
            "() ((1 2))",
        ],
    );
    let stderr = String::from_utf8_lossy(&traced.stderr).into_owned();
    assert!(traced.status.success(), "{stderr}");
    assert!(stderr.contains("Breaking stale lock"), "{stderr}");
    assert!(!lock_dir_path(&db).exists());
    assert_eq!(success(clink(&db, &["--after"])), "(1: 1 1)\n(2: 1 2)\n");
}

#[test]
fn lock_options_are_validated_before_any_store_is_created() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("unused.links");
    failure(
        clink(&db, &["--lock", "mutex"]),
        "Invalid --lock value 'mutex'. Use 'flock', 'pid' or 'none'.",
    );
    failure(
        clink(&db, &["--lock-timeout", "soon"]),
        "Invalid --lock-timeout value 'soon'",
    );
    // Reporting on a store that was never locked creates nothing either.
    let status = success(clink(&db, &["--lock-status"]));
    assert_eq!(status.matches(": unlocked\n").count(), 2, "{status}");
    assert!(entries(dir.path()).is_empty(), "{:?}", entries(dir.path()));
}

#[test]
fn explicit_side_stores_are_locked_with_the_database() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("s.links");
    let log = dir.path().join("shared-log.links");
    let held = hold_exclusive(&log);
    failure(
        clink(
            &db,
            &[
                "--transactions-file",
                &log.to_string_lossy(),
                "--lock-timeout",
                "50",
                "() ((1 1))",
            ],
        ),
        "shared-log.links.lock",
    );
    drop(held);
    success(clink(
        &db,
        &["--transactions-file", &log.to_string_lossy(), "() ((1 1))"],
    ));
}

#[test]
fn options_parse_and_stores_lock_in_one_order() {
    use link_cli::store_lock::{lock_order, StoreLockKind, StoreLockOptions};
    use std::time::Duration;
    let default = StoreLockOptions::parse(None, None).unwrap();
    assert_eq!(default.kind, StoreLockKind::Flock);
    assert_eq!(default.timeout, Duration::from_millis(30_000));
    let parsed = StoreLockOptions::parse(Some(" PID "), Some(" 0 ")).unwrap();
    assert_eq!(
        (parsed.kind, parsed.timeout),
        (StoreLockKind::Pid, Duration::ZERO)
    );
    assert_eq!(
        StoreLockOptions::parse(Some(""), None).unwrap().kind,
        StoreLockKind::Flock
    );
    assert_eq!(
        StoreLockOptions::parse(Some("none"), None).unwrap().kind,
        StoreLockKind::None
    );
    assert!(StoreLockOptions::parse(None, Some("-1")).is_err());

    let dir = tempdir().unwrap();
    let a = dir.path().join("a.links");
    let b = dir.path().join("b.links");
    // The same store spelled twice is locked once, under its first spelling.
    let again = dir.path().join(".").join("a.links");
    assert_eq!(lock_order([&b, &a, &again]), vec![a.clone(), b.clone()]);
    assert_eq!(lock_order([&again, &b]), vec![again.clone(), b.clone()]);
}
