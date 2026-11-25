//! The owner record of a store lock and the process probes that decide
//! whether its owner still runs.
//!
//! A record is a few `key=value` lines, the same in every implementation:
//!
//! ```text
//! pid=4242
//! host=build-7
//! pid_namespace=pid:[4026531836]
//! start_time=1234567
//! acquired_at_ms=1764028800000
//! token=5f0c2a9d1e3b4c6a
//! program=clink (rust) 3.1.2
//! ```
//!
//! `pid_namespace` and `start_time` are only known on Linux; elsewhere they
//! are empty and the owner is judged by host and process id alone.

use std::time::{SystemTime, UNIX_EPOCH};

/// Who holds a store lock.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LockOwner {
    pub pid: u32,
    pub host: String,
    pub pid_namespace: String,
    pub start_time: String,
    pub acquired_at_ms: u64,
    pub token: String,
    pub program: String,
}

/// Whether the owner of a lock still runs, as far as this host can tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Liveness {
    Alive,
    /// The owner is gone; the reason says how that is known.
    Gone(String),
    /// Another host or PID namespace: nothing here can tell.
    Unknown(String),
}

impl LockOwner {
    /// The record this process writes when it takes a lock.
    pub fn current(token: &str) -> Self {
        let pid = std::process::id();
        Self {
            pid,
            host: host_name(),
            pid_namespace: pid_namespace(),
            start_time: process_start_time(pid).unwrap_or_default(),
            acquired_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_millis() as u64),
            token: token.to_string(),
            program: format!("clink (rust) {}", env!("CARGO_PKG_VERSION")),
        }
    }

    /// The `key=value` lines of the record.
    pub fn to_record(&self) -> String {
        format!(
            "pid={}\nhost={}\npid_namespace={}\nstart_time={}\nacquired_at_ms={}\ntoken={}\nprogram={}\n",
            self.pid,
            self.host,
            self.pid_namespace,
            self.start_time,
            self.acquired_at_ms,
            self.token,
            self.program
        )
    }

    /// Reads a record; `None` unless it names a process id. Unknown keys are
    /// ignored so that later versions can add fields.
    pub fn parse(text: &str) -> Option<Self> {
        let mut owner = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().to_string();
            match key.trim() {
                "pid" => owner.pid = value.parse().ok()?,
                "host" => owner.host = value,
                "pid_namespace" => owner.pid_namespace = value,
                "start_time" => owner.start_time = value,
                "acquired_at_ms" => owner.acquired_at_ms = value.parse().unwrap_or(0),
                "token" => owner.token = value,
                "program" => owner.program = value,
                _ => {}
            }
        }
        (owner.pid != 0).then_some(owner)
    }

    /// One line for messages: every known field, then whether it still runs.
    pub fn describe(&self) -> String {
        let mut text = format!("pid={} host={}", self.pid, self.host);
        for (key, value) in [
            ("pid_namespace", &self.pid_namespace),
            ("start_time", &self.start_time),
            ("program", &self.program),
        ] {
            if !value.is_empty() {
                text.push_str(&format!(" {key}={value}"));
            }
        }
        if self.acquired_at_ms != 0 {
            text.push_str(&format!(" acquired_at_ms={}", self.acquired_at_ms));
        }
        match self.liveness() {
            Liveness::Alive => text.push_str(" (running)"),
            Liveness::Gone(reason) | Liveness::Unknown(reason) => {
                text.push_str(&format!(" ({reason})"))
            }
        }
        text
    }

    /// Whether the owner still runs. Only an owner on this host and in this
    /// PID namespace can be judged; a reused process id is told apart by its
    /// start time where the system reports one.
    pub fn liveness(&self) -> Liveness {
        if self.host != host_name() {
            return Liveness::Unknown(format!("held on another host, {}", self.host));
        }
        if self.pid_namespace != pid_namespace() {
            return Liveness::Unknown("held in another PID namespace".into());
        }
        match process_alive(self.pid) {
            Some(false) => Liveness::Gone(format!("owner process {} is gone", self.pid)),
            Some(true) => match process_start_time(self.pid) {
                Some(started) if !self.start_time.is_empty() && started != self.start_time => {
                    Liveness::Gone(format!(
                        "owner process {} is gone; its id now belongs to another process",
                        self.pid
                    ))
                }
                _ => Liveness::Alive,
            },
            None => Liveness::Unknown("cannot tell whether the owner runs".into()),
        }
    }
}

/// The name of this host, or an empty string when it cannot be read.
pub fn host_name() -> String {
    #[cfg(unix)]
    {
        let mut buffer = [0u8; 256];
        // SAFETY: the buffer is writable for its whole length.
        if unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) } == 0 {
            let end = buffer.iter().position(|&byte| byte == 0).unwrap_or(0);
            return String::from_utf8_lossy(&buffer[..end]).into_owned();
        }
        String::new()
    }
    #[cfg(not(unix))]
    {
        std::env::var("COMPUTERNAME").unwrap_or_default()
    }
}

/// The PID namespace of this process (Linux), so that a container does not
/// judge the process ids of another; empty elsewhere.
pub fn pid_namespace() -> String {
    #[cfg(target_os = "linux")]
    if let Ok(target) = std::fs::read_link("/proc/self/ns/pid") {
        return target.to_string_lossy().into_owned();
    }
    String::new()
}

/// When `pid` started, in clock ticks since boot (Linux); `None` elsewhere
/// or when the process does not exist.
pub fn process_start_time(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // The command name in parentheses may hold spaces; the fields after
        // it start with the state (field 3), so `starttime` (field 22) is
        // the 20th.
        let rest = &stat[stat.rfind(')')? + 1..];
        rest.split_whitespace().nth(19).map(str::to_string)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// Whether process `pid` exists; `None` when the system cannot tell.
pub fn process_alive(pid: u32) -> Option<bool> {
    if pid == 0 {
        return Some(false);
    }
    #[cfg(unix)]
    {
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return Some(false);
        };
        // SAFETY: signal 0 only checks that the process exists.
        if unsafe { libc::kill(pid, 0) } == 0 {
            return Some(true);
        }
        match errno::errno().0 {
            libc::ESRCH => Some(false),
            libc::EPERM => Some(true),
            _ => None,
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_INVALID_PARAMETER};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        const STILL_ACTIVE: u32 = 259;
        // SAFETY: the handle is checked and closed before returning.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return if GetLastError() == ERROR_INVALID_PARAMETER {
                    Some(false)
                } else {
                    // Access denied and the like: the process exists.
                    Some(true)
                };
            }
            let mut code = 0u32;
            let known = GetExitCodeProcess(handle, &mut code) != 0;
            CloseHandle(handle);
            known.then_some(code == STILL_ACTIVE)
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}
