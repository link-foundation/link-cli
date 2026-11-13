//! Endpoint strings: `[kind:]address[?name=value&...]`.

use super::KINDS;
use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::time::Duration;

/// A parsed endpoint: a kind, an address and options.
///
/// Text without a known kind prefix is a TCP `host:port`, so every address
/// accepted before IPC endpoints existed keeps its meaning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    kind: String,
    address: String,
    options: BTreeMap<String, String>,
}

const COMMON: &[&str] = &["timeout"];
const RENDEZVOUS: &[&str] = &["poll", "accept-poll"];
const RING: &[&str] = &["capacity"];
const DATAGRAM: &[&str] = &["chunk", "rto"];

fn options_of(kind: &str) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = COMMON.to_vec();
    match kind {
        "tcp" | "unix" | "stdio" | "exec" => {}
        "pipe" => {
            if cfg!(unix) {
                names.extend(RENDEZVOUS);
            }
        }
        "fifo" => names.extend(RENDEZVOUS),
        "shm" | "mmap" | "file" => {
            names.extend(RENDEZVOUS);
            names.extend(RING);
        }
        "signal" => {
            names.extend(RENDEZVOUS);
            names.extend(RING);
            names.push("signo");
        }
        "udp" => {
            names.extend(DATAGRAM);
            names.extend(["heartbeat", "dead"]);
        }
        "mq" => {
            names.extend(RENDEZVOUS);
            names.extend(DATAGRAM);
            names.push("maxmsg");
        }
        "sysvmq" | "mailslot" | "copydata" | "alpc" => {
            names.extend(RENDEZVOUS);
            names.extend(DATAGRAM);
        }
        _ => return Vec::new(),
    }
    names
}

impl Endpoint {
    /// Parses an endpoint string.
    pub fn parse(text: &str) -> io::Result<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Err(invalid("an endpoint must not be empty".to_string()));
        }
        let (kind, rest) = match text.split_once(':') {
            Some((prefix, rest)) if is_kind(prefix) => (prefix.to_ascii_lowercase(), rest),
            None if is_kind(text) => (text.to_ascii_lowercase(), ""),
            _ => ("tcp".to_string(), text),
        };
        if kind == "exec" {
            if rest.trim().is_empty() {
                return Err(invalid("`exec:` needs a command".to_string()));
            }
            return Ok(Self {
                kind,
                address: rest.to_string(),
                options: BTreeMap::new(),
            });
        }
        let (address, query) = match rest.split_once('?') {
            Some((address, query)) => (address, Some(query)),
            None => (rest, None),
        };
        let mut options = BTreeMap::new();
        if let Some(query) = query {
            let builtin = KINDS.contains(&kind.as_str());
            let allowed = options_of(&kind);
            for pair in query.split('&').filter(|pair| !pair.is_empty()) {
                let (name, value) = pair
                    .split_once('=')
                    .ok_or_else(|| invalid(format!("endpoint option `{pair}` needs a `=value`")))?;
                let name = name.to_ascii_lowercase();
                if builtin && !allowed.contains(&name.as_str()) {
                    return Err(invalid(format!(
                        "`{kind}:` endpoints have no option `{name}`{}",
                        if allowed.is_empty() {
                            String::new()
                        } else {
                            format!(" (options: {})", allowed.join(", "))
                        }
                    )));
                }
                options.insert(name, value.to_string());
            }
        }
        let endpoint = Self {
            kind,
            address: address.to_string(),
            options,
        };
        endpoint.validate()?;
        Ok(endpoint)
    }

    fn validate(&self) -> io::Result<()> {
        let needs_address = !matches!(self.kind.as_str(), "stdio");
        if needs_address && self.address.is_empty() && KINDS.contains(&self.kind.as_str()) {
            return Err(invalid(format!(
                "`{}:` endpoints need an address",
                self.kind
            )));
        }
        if self.kind == "stdio" && !self.address.is_empty() {
            return Err(invalid("`stdio` takes no address".to_string()));
        }
        for name in ["timeout", "poll", "accept-poll", "heartbeat", "dead", "rto"] {
            self.millis(name, 1)?;
        }
        self.number("capacity", 65536, 64, 1 << 30)?;
        self.number("chunk", 4096, 16, 65000)?;
        self.number("maxmsg", 8, 1, 65536)?;
        self.number("signo", 12, 1, 64)?;
        Ok(())
    }

    /// The kind, lowercase, such as `tcp` or `shm`.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Everything between the kind and the options.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The raw value of an option.
    pub fn option(&self, name: &str) -> Option<&str> {
        self.options.get(name).map(String::as_str)
    }

    /// Every option.
    pub fn options(&self) -> &BTreeMap<String, String> {
        &self.options
    }

    /// A numeric option within `min..=max`, or `default`.
    pub fn number(&self, name: &str, default: u64, min: u64, max: u64) -> io::Result<u64> {
        match self.option(name) {
            None => Ok(default),
            Some(text) => match text.parse::<u64>() {
                Ok(value) if (min..=max).contains(&value) => Ok(value),
                _ => Err(invalid(format!(
                    "endpoint option `{name}` must be a whole number from {min} to {max}, found `{text}`"
                ))),
            },
        }
    }

    /// A duration option given in milliseconds, or `default` milliseconds.
    pub fn millis(&self, name: &str, default: u64) -> io::Result<Duration> {
        self.number(name, default, 0, 86_400_000)
            .map(Duration::from_millis)
    }

    /// The connect/handshake timeout (`timeout`, 10 s by default).
    pub fn timeout(&self) -> Duration {
        self.millis("timeout", 10_000)
            .unwrap_or(Duration::from_secs(10))
            .max(Duration::from_millis(1))
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.address.is_empty() {
            write!(formatter, "{}", self.kind)?;
        } else {
            write!(formatter, "{}:{}", self.kind, self.address)?;
        }
        let mut separator = '?';
        for (name, value) in &self.options {
            write!(formatter, "{separator}{name}={value}")?;
            separator = '&';
        }
        Ok(())
    }
}

impl std::str::FromStr for Endpoint {
    type Err = io::Error;

    fn from_str(text: &str) -> io::Result<Self> {
        Self::parse(text)
    }
}

fn is_kind(prefix: &str) -> bool {
    let lower = prefix.to_ascii_lowercase();
    KINDS.contains(&lower.as_str()) || super::is_registered(&lower)
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
