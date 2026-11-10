use super::*;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn actual_rejected_growth_preserves_the_old_mapping_and_retry() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut child = Command::new("bash")
        .args(["-c", "ulimit -c 0\nulimit -v 262144\nulimit -s 8192\nulimit -t 10\ntrap '' XFSZ\nexec \"$@\"", "mapped-quota"])
        .arg(std::env::current_exe()?)
        .args(["--exact", "quota::quota_child", "--ignored", "--nocapture"])
        .env("LINK_MAPPED_GROWTH_DIRECTORY", directory.path())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            anyhow::bail!("bounded mapped growth child did not finish");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = child.wait_with_output()?;
    assert!(output.stderr.len() < 65536);
    assert!(
        status.success(),
        "mapped growth child failed: {status}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("PASS retained quota growth"));
    Ok(())
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Limit {
    current: std::ffi::c_ulong,
    maximum: std::ffi::c_ulong,
}
unsafe extern "C" {
    fn getrlimit(resource: i32, limit: *mut Limit) -> i32;
    fn setrlimit(resource: i32, limit: *const Limit) -> i32;
}
struct FileQuota(Limit);
impl FileQuota {
    fn new() -> Result<Self> {
        let mut previous = Limit {
            current: 0,
            maximum: 0,
        };
        // SAFETY: Linux RLIMIT_FSIZE uses two unsigned longs. Only this
        // isolated, core-disabled child changes its soft file-size limit.
        if unsafe { getrlimit(1, &mut previous) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let bounded = Limit {
            current: 4096,
            ..previous
        };
        if unsafe { setrlimit(1, &bounded) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self(previous))
    }
}
impl Drop for FileQuota {
    fn drop(&mut self) {
        assert_eq!(unsafe { setrlimit(1, &self.0) }, 0);
    }
}

#[test]
#[ignore = "finite parent-driven 4 KiB file quota child"]
fn quota_child() -> Result<()> {
    let Some(directory) = std::env::var_os("LINK_MAPPED_GROWTH_DIRECTORY") else {
        return Ok(());
    };
    let quota = FileQuota::new()?;
    let path = std::path::Path::new(&directory).join("retained.data");
    let mut mapped = PersistentFileMapped::<u64>::from_path(&path)?;
    mapped.grow_filled(32, 0)?[0] = 42;
    let previous = mapped.allocated().as_ptr();
    let barrier = mapped.flush_handle();
    for index in 0..3 {
        let error = mapped.grow_filled(8192, 0).unwrap_err();
        assert!(error.to_string().contains("File too large"));
        // Check the barrier first: unchanged source fails cleanly here, while
        // the separate bounded probe records its stale-pointer process signal.
        barrier.flush()?;
        assert_eq!(mapped.allocated().as_ptr(), previous);
        assert_eq!(mapped.allocated().len(), 32);
        assert_eq!(mapped.allocated()[0], 42);
        mapped.allocated_mut()[1] = index + 73;
        barrier.flush()?;
    }
    drop(quota);
    assert_eq!(mapped.grow_filled(8192, 19)?, [19; 8192]);
    barrier.flush()?;
    mapped.shrink(8192)?;
    barrier.flush()?;
    drop(mapped);
    let reopened = PersistentFileMapped::<u64>::open_existing(
        std::fs::File::options()
            .read(true)
            .write(true)
            .open(&path)?,
    )?;
    assert_eq!(reopened.allocated().len(), 32);
    assert_eq!(&reopened.allocated()[..2], [42, 75]);
    reopened.flush_handle().flush()?;
    eprintln!("PASS retained quota growth: repeated errors, exact pointer, writes, flush, retry and reopen");
    Ok(())
}
