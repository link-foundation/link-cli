//! Actual Linux quota failures leave a partial line on the live writer.
#![cfg(target_os = "linux")]
use anyhow::{bail, Result};
use link_cli::{FileTransitionLog, TransitionLogStore};
use std::{
    ffi::c_ulong,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn verify(mode: &str, unicode: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut child = Command::new("bash")
        .args(["-c", "trap '' XFSZ\nexec \"$@\"", "quota-child"])
        .arg(std::env::current_exe()?)
        .args(["--exact", "quota_child", "--ignored", "--nocapture"])
        .env("LINK_TAIL_QUOTA_DIRECTORY", directory.path())
        .env("LINK_TAIL_QUOTA_MODE", mode)
        .env("LINK_TAIL_QUOTA_UNICODE", unicode.to_string())
        .stdout(Stdio::null())
        // A regular parent log can already exceed the child's file quota.
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            bail!("finite quota child did not finish: {mode}/{unicode}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = child.wait_with_output()?;
    assert!(
        status.success(),
        "quota child failed at {mode}/{unicode}: {status}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut reopened = FileTransitionLog::open(directory.path().join("journal"))?;
    assert_eq!(reopened.read_log_entries()?, ["committed", "following"]);
    Ok(())
}
macro_rules! case {
    ($name:ident, $mode:literal, $unicode:literal) => {
        #[test]
        fn $name() -> Result<()> {
            verify($mode, $unicode)
        }
    };
}
case!(read_after_partial_ascii, "read", false);
case!(read_after_partial_utf8, "read", true);
case!(append_after_partial_ascii, "append", false);
case!(append_after_partial_utf8, "append", true);
case!(flush_after_partial_ascii, "flush", false);
case!(flush_after_partial_utf8, "flush", true);
case!(repair_retry_after_partial_ascii, "retry", false);
case!(repair_retry_after_partial_utf8, "retry", true);

#[test]
#[ignore = "finite 8 KiB quota child entry point"]
fn quota_child() -> Result<()> {
    let Some(directory) = std::env::var_os("LINK_TAIL_QUOTA_DIRECTORY") else {
        return Ok(());
    };
    let _quota = FileSizeQuota::new()?;
    let path = Path::new(&directory).join("journal");
    let mut log = FileTransitionLog::open(&path)?;
    log.append_log_entry("committed")?;
    let unicode = std::env::var("LINK_TAIL_QUOTA_UNICODE")? == "true";
    let candidate = if unicode {
        "雪".repeat(6667)
    } else {
        "x".repeat(20000)
    };
    let error = log.append_log_entry(&candidate).unwrap_err();
    eprintln!("Expected actual quota failure: {error}");
    let length = std::fs::metadata(&path)?.len();
    assert!(
        length > 10 && length <= 8192,
        "expected bounded partial write, got {length}"
    );
    match std::env::var("LINK_TAIL_QUOTA_MODE")?.as_str() {
        "read" => assert_eq!(log.read_log_entries()?, ["committed"]),
        "flush" => {
            log.flush_log()?;
            assert_eq!(log.read_log_entries()?, ["committed"]);
        }
        "retry" => {
            let moved = path.with_extension("moved");
            std::fs::rename(&path, &moved)?;
            assert!(log.read_log_entries().is_err());
            assert!(log.append_log_entry("blocked").is_err());
            assert!(log.flush_log().is_err());
            std::fs::rename(&moved, &path)?;
            assert_eq!(log.read_log_entries()?, ["committed"]);
        }
        _ => (),
    }
    log.append_log_entry("following")?;
    log.flush_log()?;
    assert_eq!(log.read_log_entries()?, ["committed", "following"]);
    Ok(())
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ResourceLimit {
    current: c_ulong,
    maximum: c_ulong,
}
unsafe extern "C" {
    fn getrlimit(resource: i32, limit: *mut ResourceLimit) -> i32;
    fn setrlimit(resource: i32, limit: *const ResourceLimit) -> i32;
}

struct FileSizeQuota(ResourceLimit);
impl FileSizeQuota {
    fn new() -> Result<Self> {
        let mut original = ResourceLimit {
            current: 0,
            maximum: 0,
        };
        // Linux RLIMIT_FSIZE is 1; the repr(C) fields match unsigned long rlim_t.
        if unsafe { getrlimit(1, &mut original) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let bounded = ResourceLimit {
            current: 8192,
            ..original
        };
        if unsafe { setrlimit(1, &bounded) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self(original))
    }
}
impl Drop for FileSizeQuota {
    fn drop(&mut self) {
        // Restore before LLVM writes its larger profile during process exit,
        // including unwinding after a failed journal assertion.
        assert_eq!(unsafe { setrlimit(1, &self.0) }, 0);
    }
}
