use super::super::file::events;
use super::*;
use std::{
    cell::RefCell,
    fs,
    process::{Command, Stdio},
    rc::Rc,
    time::{Duration, Instant},
};

fn records() -> Vec<String> {
    vec!["雪".repeat(700), "\u{feff}\0😀".into()]
}
fn seed(path: &Path, mirrored: bool, repair: bool) {
    let mut log = FileCheckpointPagedLog::open(path, mirrored).unwrap();
    log.replace_log_entries(&["retired bank".into()]).unwrap();
    log.replace_log_entries(&["acknowledged".into()]).unwrap();
    log.close().unwrap();
    if repair {
        fs::OpenOptions::new()
            .write(true)
            .open(suffix(path, ".bank0"))
            .unwrap()
            .set_len(19)
            .unwrap();
        fs::write(suffix(path, ".bank0.meta0"), b"torn").unwrap();
    }
}
fn child(path: &Path, mirrored: bool, point: isize, repair: bool) -> i32 {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "engine::paged::file_checkpoint::process_tests::native_checkpoint_cut_child",
            "--ignored",
            "--nocapture",
        ])
        .env("CLINK_CHECKPOINT_CUT_PATH", path)
        .env("CLINK_CHECKPOINT_CUT_MIRROR", mirrored.to_string())
        .env("CLINK_CHECKPOINT_CUT_POINT", point.to_string())
        .env("CLINK_CHECKPOINT_CUT_REPAIR", repair.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: only async-signal-safe setrlimit calls run before exec.
        unsafe {
            command.pre_exec(|| {
                let stack = libc::rlimit {
                    rlim_cur: 8 * 1024 * 1024,
                    rlim_max: 8 * 1024 * 1024,
                };
                if libc::setrlimit(libc::RLIMIT_STACK, &stack) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status.code().expect("child terminated normally");
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("bounded native checkpoint child expired");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn every_real_native_replacement_mutation_survives_before_and_after_process_exit() {
    for (mirrored, repair) in [(false, false), (true, false), (true, true)] {
        let dir = tempfile::tempdir().unwrap();
        let baseline = dir.path().join("baseline");
        seed(&baseline, mirrored, repair);
        assert_eq!(child(&baseline, mirrored, -1, repair), 0);
        let trace = fs::read_to_string(suffix(&baseline, ".events")).unwrap();
        let points: Vec<_> = trace.lines().collect();
        assert!(
            (if repair { 12..=150 } else { 30..=150 }).contains(&points.len()),
            "finite native mutation trace: {trace}"
        );
        for operation in ["write", "truncate", "sync"] {
            assert!(trace.contains(operation));
        }
        assert!(trace.contains(if repair {
            "journal.bank0.writer:0:repair"
        } else {
            "journal.writer:2:write"
        }));
        for (point, operation) in points.iter().enumerate() {
            let case = tempfile::tempdir().unwrap();
            let path = case.path().join("journal");
            seed(&path, mirrored, repair);
            assert_eq!(
                child(&path, mirrored, point as isize, repair),
                73,
                "point {point}: {}",
                operation
            );
            assert!(FileCheckpointPagedLog::open(&path, mirrored).is_err());
            // Only the observed child exit permits explicit owner removal.
            for name in super::tests::names(&path, mirrored) {
                fs::remove_file(suffix(&name, ".writer")).unwrap();
            }
            let mut recovered = FileCheckpointPagedLog::open(&path, mirrored).unwrap();
            let actual = recovered.read_log_entries().unwrap();
            assert!(
                actual == ["acknowledged"] || actual == records(),
                "torn result at point {point}: {}",
                operation
            );
            if repair {
                assert_eq!(actual, ["acknowledged"]);
            }
            recovered.append_log_entry("following").unwrap();
            recovered.flush_log().unwrap();
            assert_eq!(
                recovered.read_log_entries().unwrap(),
                [actual, vec!["following".into()]].concat()
            );
            recovered
                .replace_log_entries(&["compacted after exit".into()])
                .unwrap();
            recovered.close().unwrap();
            let mut again = FileCheckpointPagedLog::open(&path, mirrored).unwrap();
            assert_eq!(again.read_log_entries().unwrap(), ["compacted after exit"]);
            again.close().unwrap();
        }
        eprintln!(
            "native checkpoint mirrored={mirrored} repair={repair}: {} bounded before/after process exits",
            points.len()
        );
    }
}

#[test]
#[ignore = "finite child entry exercised by every_real_native_replacement_mutation_survives_before_and_after_process_exit"]
fn native_checkpoint_cut_child() {
    let path = PathBuf::from(std::env::var_os("CLINK_CHECKPOINT_CUT_PATH").unwrap());
    let mirrored = std::env::var("CLINK_CHECKPOINT_CUT_MIRROR")
        .unwrap()
        .parse()
        .unwrap();
    let point: isize = std::env::var("CLINK_CHECKPOINT_CUT_POINT")
        .unwrap()
        .parse()
        .unwrap();
    let trace = Rc::new(RefCell::new(Vec::new()));
    let observed = trace.clone();
    let repair: bool = std::env::var("CLINK_CHECKPOINT_CUT_REPAIR")
        .unwrap()
        .parse()
        .unwrap();
    let observer = Box::new(move |name: &Path, file, operation: &str, after| {
        let mut trace = observed.borrow_mut();
        // Normalize the root name so a baseline and each fresh case compare.
        let name = name.file_name().unwrap().to_string_lossy();
        let name = name
            .strip_prefix("baseline")
            .map_or_else(|| name.to_string(), |rest| format!("journal{rest}"));
        trace.push(format!("{name}:{file}:{operation}:{after}"));
        if point == trace.len() as isize - 1 {
            std::process::exit(73);
        }
    });
    let mut log = if repair {
        events::set(Some(observer));
        FileCheckpointPagedLog::open(&path, mirrored).unwrap()
    } else {
        let log = FileCheckpointPagedLog::open(&path, mirrored).unwrap();
        events::set(Some(observer));
        log
    };
    if !repair {
        log.replace_log_entries(&records()).unwrap();
    }
    events::set(None);
    log.close().unwrap();
    fs::write(suffix(&path, ".events"), trace.borrow().join("\n")).unwrap();
}
