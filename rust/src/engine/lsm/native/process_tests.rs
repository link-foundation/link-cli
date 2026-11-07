use super::{test_support::*, *};
use crate::engine::paged::events;
use std::{
    cell::Cell,
    fs,
    path::Path,
    process::{Command, Stdio},
    rc::Rc,
    time::{Duration, Instant},
};

fn child(path: &Path, mode: usize, point: usize, phase: &str) -> i32 {
    let stderr_path = path.parent().unwrap().join("child-stderr.log");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "engine::lsm::native::process_tests::native_lsm_exit_child",
            "--ignored",
            "--nocapture",
        ])
        .env("CLINK_NATIVE_LSM_PATH", path)
        .env("CLINK_NATIVE_LSM_MODE", mode.to_string())
        .env("CLINK_NATIVE_LSM_POINT", point.to_string())
        .env("CLINK_NATIVE_LSM_PHASE", phase)
        .stdout(Stdio::null())
        .stderr(Stdio::from(fs::File::create(&stderr_path).unwrap()));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: only async-signal-safe setrlimit calls run between fork and exec.
        unsafe {
            command.pre_exec(|| {
                let stack = libc::rlimit {
                    rlim_cur: 8 * 1024 * 1024,
                    rlim_max: 8 * 1024 * 1024,
                };
                if libc::setrlimit(libc::RLIMIT_STACK, &stack) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                #[cfg(target_os = "linux")]
                {
                    let memory = libc::rlimit {
                        rlim_cur: 512 * 1024 * 1024,
                        rlim_max: 512 * 1024 * 1024,
                    };
                    if libc::setrlimit(libc::RLIMIT_AS, &memory) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            if status.code() != Some(73) {
                use std::io::Read;
                let mut error = String::new();
                fs::File::open(stderr_path)
                    .unwrap()
                    .take(4096)
                    .read_to_string(&mut error)
                    .unwrap();
                eprintln!("native child mode {mode} {phase} point {point}: {error}");
            }
            return status.code().expect("native child exited normally");
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("finite native LSM child deadline");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn process_cuts(mode: usize) {
    let baseline = tempfile::tempdir().unwrap();
    let path = baseline.path().join("store");
    fs::create_dir(&path).unwrap();
    seed(&path, mode);
    let mut store = FileLsmJournalStore::open(&path, options(mode)).unwrap();
    let expected = values(&mut store);
    let trace = Trace::new(&path, None);
    store.compact().unwrap();
    trace.stop();
    store.close().unwrap();
    let boundaries = trace.events.borrow().clone();
    assert!((60..=512).contains(&boundaries.len()));
    for (point, event) in boundaries.iter().enumerate() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("store");
        fs::create_dir(&path).unwrap();
        seed(&path, mode);
        assert_eq!(
            child(&path, mode, point, "compact"),
            73,
            "point {point}: {event:?}"
        );
        assert!(path.join(".lsm-writer").exists());
        assert!(FileLsmJournalStore::open(&path, options(mode)).is_err());
        // Reaped child exit is the evidence required for external marker recovery.
        recover(&path);
        let mut recovered = FileLsmJournalStore::open(&path, options(mode)).unwrap();
        assert_eq!(values(&mut recovered), expected, "point {point}: {event:?}");
        put(&mut recovered, "d", "following");
        recovered.flush().unwrap();
        recovered.compact().unwrap();
        recovered.close().unwrap();
        let mut again = FileLsmJournalStore::open(&path, options(mode)).unwrap();
        assert_eq!(again.read(&["d".into()]).unwrap()[0].entries, ["following"]);
        again.close().unwrap();
    }
    eprintln!(
        "native Rust mode {mode}: {} reaped compaction before/after process exits",
        boundaries.len()
    );
}
#[test]
fn native_lsm_compaction_process_exits_plain() {
    process_cuts(0);
}
#[test]
fn native_lsm_compaction_process_exits_mirrored() {
    process_cuts(7);
}

#[test]
#[ignore = "finite child entry exercised by native LSM process-exit controls"]
fn native_lsm_exit_child() {
    let path = std::path::PathBuf::from(std::env::var_os("CLINK_NATIVE_LSM_PATH").unwrap());
    let mode = std::env::var("CLINK_NATIVE_LSM_MODE")
        .unwrap()
        .parse()
        .unwrap();
    let point: usize = std::env::var("CLINK_NATIVE_LSM_POINT")
        .unwrap()
        .parse()
        .unwrap();
    let phase = std::env::var("CLINK_NATIVE_LSM_PHASE").unwrap();
    let mut store = FileLsmJournalStore::open(path, options(mode)).unwrap();
    let count = Rc::new(Cell::new(0));
    events::set_failure(Some(Box::new(move |_, _, _, _| {
        let at = count.get();
        count.set(at + 1);
        if at == point {
            std::process::exit(73);
        }
        Ok(())
    })));
    assert_eq!(phase, "compact");
    store.compact().unwrap();
    panic!("requested finite native cut was not reached");
}
