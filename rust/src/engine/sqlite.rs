//! Optional SQLite page-WAL component at the ordered revision-log boundary.
//! A dedicated connection belongs to this log. Drop rolls pending records back.
//! SQLite writer locking does not coordinate independent cached engine states;
//! stale-tail/component errors fence this owner until it is dropped and reopened.
use crate::{LinkError, TransitionLogStore};
use rusqlite::{params, Connection};
use std::path::Path;
use std::time::Duration;

#[cfg(test)]
#[path = "sqlite_tests.rs"]
mod tests;

const APPLICATION_ID: i64 = 0x4c4e4b4a;
const SCHEMA: &str = "CREATE TABLE link_cli_log (sequence INTEGER PRIMARY KEY CHECK(sequence>0), record BLOB NOT NULL CHECK(typeof(record)='blob'))";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqliteOptions {
    /// Finite SQLite busy retry interval; zero fails immediately.
    pub busy_timeout_ms: u32,
    /// WAL pages before SQLite attempts an automatic checkpoint; zero is manual.
    pub auto_checkpoint_pages: u32,
}
impl Default for SqliteOptions {
    fn default() -> Self {
        Self {
            busy_timeout_ms: 0,
            auto_checkpoint_pages: 1000,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointMode {
    Passive,
    Full,
    Restart,
    Truncate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointResult {
    pub busy: i64,
    pub log_frames: i64,
    pub checkpointed_frames: i64,
}

#[derive(Debug)]
pub struct SqliteLog {
    connection: Connection,
    tail: i64,
    pending: bool,
    poisoned: bool,
}
fn error(message: impl std::fmt::Display) -> LinkError {
    LinkError::StorageError(message.to_string())
}

impl SqliteLog {
    /// Opens a dedicated database. Default builds have no SQLite dependency.
    /// Requires a same-host filesystem/VFS implementing SQLite WAL locking and
    /// truthful synchronization; all sidecars belong to the database.
    pub fn open(path: impl AsRef<Path>, options: SqliteOptions) -> Result<Self, LinkError> {
        if options.busy_timeout_ms > i32::MAX as u32
            || options.auto_checkpoint_pages > i32::MAX as u32
        {
            return Err(error(
                "SQLite options must fit nonnegative signed 32-bit integers",
            ));
        }
        let mut log = Self {
            connection: Connection::open(path).map_err(error)?,
            tail: 0,
            pending: false,
            poisoned: false,
        };
        log.initialize(options)?;
        Ok(log)
    }
    fn initialize(&mut self, options: SqliteOptions) -> Result<(), LinkError> {
        if rusqlite::version_number() < 3_051_003 {
            return Err(error("SQLite >= 3.51.3 is required"));
        }
        self.identity()?; // Do not change a foreign database's journal mode.
        self.connection
            .busy_timeout(Duration::from_millis(options.busy_timeout_ms.into()))
            .map_err(error)?;
        let mode: String = self
            .connection
            .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
            .map_err(error)?;
        if mode != "wal" {
            return Err(error("SQLite journal requires file-backed WAL"));
        }
        self.connection
            .execute_batch(&format!(
                "PRAGMA synchronous=FULL; PRAGMA wal_autocheckpoint={}",
                options.auto_checkpoint_pages
            ))
            .map_err(error)?;
        self.settings()?;
        if self.scalar("PRAGMA busy_timeout")? != i64::from(options.busy_timeout_ms)
            || self.scalar("PRAGMA wal_autocheckpoint")? != i64::from(options.auto_checkpoint_pages)
        {
            return Err(error("SQLite options were not applied"));
        }
        self.connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(error)?;
        self.pending = true;
        if self.identity()? {
            self.connection
                .execute_batch(&format!(
                    "{SCHEMA}; PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version=1"
                ))
                .map_err(error)?;
        }
        let (count, first, last): (i64, i64, i64) = self.connection.query_row(
            "SELECT COUNT(*), COALESCE(MIN(sequence),0), COALESCE(MAX(sequence),0) FROM link_cli_log", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).map_err(error)?;
        if count < 0 || count != last || first != if count == 0 { 0 } else { 1 } {
            return Err(error("journal sequences must be contiguous from one"));
        }
        self.tail = last;
        self.connection.execute_batch("COMMIT").map_err(error)?;
        self.pending = false;
        Ok(())
    }
    fn scalar(&self, query: &str) -> Result<i64, LinkError> {
        self.connection
            .query_row(query, [], |row| row.get(0))
            .map_err(error)
    }
    fn identity(&self) -> Result<bool, LinkError> {
        let id = self.scalar("PRAGMA application_id")?;
        let version = self.scalar("PRAGMA user_version")?;
        let mut statement = self
            .connection
            .prepare("SELECT type, name, tbl_name, sql FROM sqlite_schema ORDER BY name")
            .map_err(error)?;
        let objects: Vec<(String, String, String, Option<String>)> = statement
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .map_err(error)?
            .collect::<Result<_, _>>()
            .map_err(error)?;
        if id == 0 && version == 0 && objects.is_empty() {
            return Ok(true);
        }
        if id != APPLICATION_ID
            || version != 1
            || objects.len() != 1
            || objects[0]
                != (
                    "table".into(),
                    "link_cli_log".into(),
                    "link_cli_log".into(),
                    Some(SCHEMA.into()),
                )
        {
            return Err(error("foreign or unsupported journal schema"));
        }
        Ok(false)
    }
    fn settings(&self) -> Result<(), LinkError> {
        let mode: String = self
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(error)?;
        if mode != "wal" || self.scalar("PRAGMA synchronous")? != 2 {
            return Err(error("SQLite WAL/FULL settings changed"));
        }
        Ok(())
    }
    fn check(&self) -> Result<(), LinkError> {
        if self.poisoned {
            return Err(error("indeterminate journal outcome; drop and reopen"));
        }
        Ok(())
    }
    fn rollback(&mut self) {
        if self.pending {
            // A failed COMMIT can already have committed. Preserve the original
            // error and release a still-active transaction without deleting rows.
            let _ = self.connection.execute_batch("ROLLBACK");
            self.pending = false;
        }
    }
    fn io<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<T, LinkError>,
    ) -> Result<T, LinkError> {
        match operation(self) {
            Ok(value) => Ok(value),
            Err(failure) => {
                self.poisoned = true;
                self.rollback();
                Err(failure)
            }
        }
    }
    fn flush_with(
        &mut self,
        commit: impl FnOnce(&Connection) -> Result<(), LinkError>,
    ) -> Result<(), LinkError> {
        self.check()?;
        if !self.pending {
            return Ok(());
        }
        self.io(|log| {
            log.settings()?;
            commit(&log.connection)?;
            log.pending = false;
            Ok(())
        })
    }
    /// Explicit physical page maintenance. Logical journal records are retained.
    /// Pending appends reject without poisoning. Reader contention is returned
    /// in the counters, bounded by the configured SQLite busy interval.
    pub fn checkpoint(&mut self, mode: CheckpointMode) -> Result<CheckpointResult, LinkError> {
        self.check()?;
        if self.pending {
            return Err(error("cannot checkpoint pending appends"));
        }
        self.io(|log| {
            log.settings()?;
            let mode = match mode {
                CheckpointMode::Passive => "PASSIVE",
                CheckpointMode::Full => "FULL",
                CheckpointMode::Restart => "RESTART",
                CheckpointMode::Truncate => "TRUNCATE",
            };
            log.connection
                .query_row(&format!("PRAGMA wal_checkpoint({mode})"), [], |row| {
                    Ok(CheckpointResult {
                        busy: row.get(0)?,
                        log_frames: row.get(1)?,
                        checkpointed_frames: row.get(2)?,
                    })
                })
                .map_err(error)
        })
    }
}
impl TransitionLogStore for SqliteLog {
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.check()?;
        self.io(|log| {
            let mut statement = log
                .connection
                .prepare("SELECT sequence, record FROM link_cli_log ORDER BY sequence")
                .map_err(error)?;
            let mut rows = statement.query([]).map_err(error)?;
            let mut records = Vec::new();
            let mut previous = 0i64;
            while let Some(row) = rows.next().map_err(error)? {
                let current: i64 = row.get(0).map_err(error)?;
                if previous.checked_add(1) != Some(current) {
                    return Err(error("journal sequences must be contiguous from one"));
                }
                let bytes: Vec<u8> = row.get(1).map_err(error)?;
                let record = String::from_utf8(bytes).map_err(error)?;
                if record.contains(['\r', '\n']) {
                    return Err(error("journal record contains a physical line break"));
                }
                records.push(record);
                previous = current;
            }
            if previous != log.tail {
                return Err(error("stale journal owner: sequence tail changed"));
            }
            Ok(records)
        })
    }
    fn append_log_entry(&mut self, record: &str) -> Result<(), LinkError> {
        self.check()?;
        if record.contains(['\r', '\n']) {
            return Err(error("journal records must not contain line breaks"));
        }
        let next = self
            .tail
            .checked_add(1)
            .ok_or_else(|| error("journal sequence exhausted"))?;
        self.io(|log| {
            if !log.pending {
                log.connection
                    .execute_batch("BEGIN IMMEDIATE")
                    .map_err(error)?;
                log.pending = true;
                log.settings()?;
                if log.scalar("SELECT COALESCE(MAX(sequence),0) FROM link_cli_log")? != log.tail {
                    return Err(error("stale journal owner: sequence tail changed"));
                }
            }
            log.connection
                .execute(
                    "INSERT INTO link_cli_log (sequence, record) VALUES (?1, ?2)",
                    params![next, record.as_bytes()],
                )
                .map_err(error)?;
            log.tail = next;
            Ok(())
        })
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.flush_with(|connection| connection.execute_batch("COMMIT").map_err(error))
    }
}
impl Drop for SqliteLog {
    fn drop(&mut self) {
        self.rollback();
    }
}
