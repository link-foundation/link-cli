//! Finite phase progress, separate from persistence decisions under test.
use anyhow::{bail, Result};
use std::{
    fs::File,
    io::Read,
    path::Path,
    process::{Child, ExitStatus},
    time::{Duration, Instant},
};
pub const MAX_PHASES: usize = 64;

fn completed(bytes: &[u8]) -> Result<usize> {
    if bytes.len() > MAX_PHASES * 3 {
        bail!("Movement child exceeded finite progress bound");
    }
    let mut count = 0;
    for part in bytes.split_inclusive(|byte| *byte == b'\n') {
        if part.last() != Some(&b'\n') {
            break;
        } // Concurrent append may expose a partial final line.
        count += 1;
        if count > MAX_PHASES || part != format!("{count}\n").as_bytes() {
            bail!("Invalid movement phase sequence");
        }
    }
    Ok(count)
}
struct Watch {
    phases: usize,
    changed: Instant,
    idle: Duration,
}
impl Watch {
    fn observe(&mut self, bytes: &[u8], now: Instant) -> Result<()> {
        let phases = completed(bytes)?;
        if phases < self.phases {
            bail!("Movement phase sequence regressed");
        }
        if phases > self.phases {
            self.phases = phases;
            self.changed = now;
        }
        if now.duration_since(self.changed) >= self.idle {
            bail!("Movement child stopped making phase progress");
        }
        Ok(())
    }
}
pub fn wait(child: &mut Child, path: &Path, idle: Duration) -> Result<ExitStatus> {
    let mut watch = Watch {
        phases: 0,
        changed: Instant::now(),
        idle,
    };
    let result = (|| loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        let mut bytes = Vec::new();
        match File::open(path) {
            Ok(file) => {
                file.take((MAX_PHASES * 3 + 1) as u64)
                    .read_to_end(&mut bytes)?;
            }
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {}
            Err(cause) => return Err(cause.into()),
        }
        watch.observe(&bytes, Instant::now())?;
        std::thread::sleep(Duration::from_millis(10));
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}
#[test]
fn finite_phase_watch_admits_slow_total_work_but_rejects_stalled_or_repeated_progress() -> Result<()>
{
    let start = Instant::now();
    let idle = Duration::from_secs(10);
    let mut watch = Watch {
        phases: 0,
        changed: start,
        idle,
    };
    watch.observe(b"1\n", start + Duration::from_secs(8))?;
    watch.observe(b"1\n2\n", start + Duration::from_secs(16))?;
    watch.observe(b"1\n2\n3", start + Duration::from_secs(24))?;
    // A repeated or incomplete progress line never extends the idle deadline.
    assert!(watch
        .observe(b"1\n2\n3", start + Duration::from_secs(26))
        .is_err());
    assert!(watch
        .observe(b"1\n", start + Duration::from_secs(27))
        .is_err());
    Ok(())
}
#[test]
fn progress_parser_enforces_a_finite_monotonic_sequence() -> Result<()> {
    assert_eq!(completed(b"")?, 0);
    assert_eq!(completed(b"1")?, 0);
    let full = (1..=MAX_PHASES)
        .map(|n| format!("{n}\n"))
        .collect::<String>();
    assert_eq!(completed(full.as_bytes())?, MAX_PHASES);
    for bad in [b"0\n".as_slice(), b"1\n1\n", b"2\n", b"01\n", b"x\n"] {
        assert!(completed(bad).is_err());
    }
    assert!(completed(format!("{full}65\n").as_bytes()).is_err());
    assert!(completed(&[b'x'; MAX_PHASES * 3 + 1]).is_err());
    Ok(())
}

#[test]
fn every_supervisor_error_kills_and_reaps_its_child() -> Result<()> {
    use std::process::{Command, Stdio};
    let directory = tempfile::tempdir()?;
    for (name, bytes, idle) in [
        ("invalid", b"2\n".as_slice(), Duration::from_secs(10)),
        ("stalled", b"".as_slice(), Duration::ZERO),
    ] {
        let path = directory.path().join(name);
        std::fs::write(&path, bytes)?;
        let mut child = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "movement_progress::bounded_wait_child",
                "--ignored",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        assert!(wait(&mut child, &path, idle).is_err());
        assert!(child.try_wait()?.is_some(), "{name}: unreaped child");
    }
    // A directory cannot be consumed as a progress file on any native platform.
    let mut child = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "movement_progress::bounded_wait_child",
            "--ignored",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    assert!(wait(&mut child, directory.path(), Duration::from_secs(10)).is_err());
    assert!(child.try_wait()?.is_some(), "I/O error: unreaped child");
    Ok(())
}
#[test]
#[ignore = "Finite child for supervisor cleanup controls"]
fn bounded_wait_child() {
    std::thread::sleep(Duration::from_secs(1));
}
