//! Ends a served CLI the way a supervisor does, so the test leaves the process
//! exiting through its normal path. Windows has no such signal to send: there the
//! process is killed.

use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// Sends the termination signal and returns how the process ended, or `None` when
/// it had to be killed.
pub fn terminate(child: &mut Child) -> Option<std::process::ExitStatus> {
    if let Ok(Some(status)) = child.try_wait() {
        return Some(status);
    }
    let signalled = cfg!(unix)
        && Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .is_ok_and(|status| status.success());
    if signalled {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(_) => break,
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}
