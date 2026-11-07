//! Bounded waiting and deadlock refusal of pessimistic point writes. Native only:
//! a wasm32 thread cannot wait for another; the JS client waits between requests.
#![cfg(not(target_arch = "wasm32"))]
use link_cli::engine::{ConcurrencyControl, Engine, EngineOptions, IsolationLevel, MemoryLog};
use link_cli::{LinkError, LinksStorage, LinoLink, NamedTypeLinks, QueryProcessor};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

const LONG: Duration = Duration::from_secs(60);

fn waiting(wait: Duration) -> Engine<u32, MemoryLog> {
    let db = Engine::open(
        MemoryLog::default(),
        EngineOptions {
            concurrency: ConcurrencyControl::Pessimistic,
            isolation: IsolationLevel::Snapshot,
            lock_wait: Some(wait),
            ..Default::default()
        },
    )
    .unwrap();
    let mut seed = db.begin().unwrap();
    for id in 1..=3 {
        seed.ensure_link_created(id).unwrap();
    }
    seed.commit().unwrap();
    db
}

/// Runs `write` on a new session in a thread, and returns once it is waiting: the
/// `started` signal comes just before the write, and the short sleep lets it block.
fn write_in_thread<F>(
    db: &Engine<u32, MemoryLog>,
    write: F,
) -> thread::JoinHandle<Result<(), LinkError>>
where
    F: FnOnce(&mut link_cli::engine::EngineTransaction<u32, MemoryLog>) -> Result<(), LinkError>
        + Send
        + 'static,
{
    let db = db.clone();
    let (started, ready) = mpsc::channel();
    let handle = thread::spawn(move || {
        let mut session = db.begin()?;
        started.send(()).unwrap();
        write(&mut session)?;
        session.commit().map(|_| ())
    });
    ready.recv().unwrap();
    thread::sleep(Duration::from_millis(50));
    handle
}

#[test]
fn a_write_waits_for_the_owner_to_roll_back_and_then_succeeds() {
    let db = waiting(LONG);
    let mut owner = db.begin().unwrap();
    owner.set_name(1, Some("private".into())).unwrap();
    let waiter = write_in_thread(&db, |session| session.set_name(1, Some("next".into())));
    assert!(!waiter.is_finished());
    owner.rollback();
    waiter.join().unwrap().unwrap();
    assert_eq!(db.begin().unwrap().get_by_name("next"), Some(1));
}

#[test]
fn a_write_that_waited_for_a_commit_is_a_conflict_like_any_stale_write() {
    let db = waiting(LONG);
    let mut owner = db.begin().unwrap();
    owner.set_name(1, Some("first".into())).unwrap();
    let mut waiter = db.begin().unwrap();
    let handle = thread::spawn(move || waiter.set_name(1, Some("second".into())));
    thread::sleep(Duration::from_millis(50));
    assert!(!handle.is_finished());
    owner.commit().unwrap();
    let refused = handle.join().unwrap().unwrap_err().to_string();
    assert!(refused.contains("conflict"), "{refused}");
    assert_eq!(db.begin().unwrap().get_by_name("first"), Some(1));
}

#[test]
fn a_wait_ends_with_busy_at_its_limit_and_keeps_earlier_staged_writes() {
    let db = waiting(Duration::from_millis(30));
    let mut owner = db.begin().unwrap();
    owner.set_name(1, Some("owned".into())).unwrap();
    let mut waiter = db.begin().unwrap();
    waiter.set_name(2, Some("kept".into())).unwrap();
    let started = Instant::now();
    let refused = waiter.set_name(1, None).unwrap_err().to_string();
    assert!(started.elapsed() >= Duration::from_millis(30));
    assert!(
        refused.starts_with("Transaction error: busy") || refused.contains("busy"),
        "{refused}"
    );
    assert_eq!(waiter.get_by_name("kept"), Some(2));
    owner.rollback();
    waiter.set_name(1, Some("after".into())).unwrap();
    waiter.commit().unwrap();
}

#[test]
fn a_zero_wait_answers_at_once() {
    let db = waiting(Duration::ZERO);
    let mut owner = db.begin().unwrap();
    owner.set_name(1, Some("owned".into())).unwrap();
    let mut other = db.begin().unwrap();
    assert!(other
        .update_link(1, 1, 1)
        .unwrap_err()
        .to_string()
        .contains("busy"));
    assert_eq!(
        other.take_write_blocker(),
        None,
        "a wait ends before the owner refuses"
    );
    // The owner may repeat its own write without waiting for itself.
    owner.set_name(1, Some("again".into())).unwrap();
}

#[test]
fn the_write_that_closes_a_cycle_is_refused_and_the_other_continues() {
    let db = waiting(LONG);
    let mut first = db.begin().unwrap();
    let mut second = db.begin().unwrap();
    first.set_name(1, Some("one".into())).unwrap();
    second.set_name(2, Some("two".into())).unwrap();
    let handle = thread::spawn(move || {
        first.set_name(2, Some("first".into()))?;
        first.commit()
    });
    thread::sleep(Duration::from_millis(50));
    let started = Instant::now();
    let refused = second.set_name(1, None).unwrap_err().to_string();
    assert!(refused.contains("deadlock"), "{refused}");
    assert!(
        started.elapsed() < LONG / 2,
        "the refusal does not wait for the limit"
    );
    assert_eq!(
        second.get_by_name("two"),
        Some(2),
        "the session stays usable"
    );
    second.rollback();
    handle.join().unwrap().unwrap();
    assert_eq!(db.begin().unwrap().get_by_name("first"), Some(2));
}

#[test]
fn a_cycle_through_three_sessions_is_refused() {
    let db = waiting(LONG);
    let mut first = db.begin().unwrap();
    let mut second = db.begin().unwrap();
    let mut third = db.begin().unwrap();
    first.set_name(1, Some("one".into())).unwrap();
    second.set_name(2, Some("two".into())).unwrap();
    third.set_name(3, Some("three".into())).unwrap();
    let a = thread::spawn(move || {
        let result = first.set_name(2, None);
        first.rollback();
        result
    });
    thread::sleep(Duration::from_millis(50));
    let b = thread::spawn(move || {
        let result = second.set_name(3, None);
        second.rollback();
        result
    });
    thread::sleep(Duration::from_millis(50));
    let refused = third.set_name(1, None).unwrap_err().to_string();
    assert!(refused.contains("deadlock"), "{refused}");
    third.rollback();
    b.join().unwrap().unwrap();
    a.join().unwrap().unwrap();
}

#[test]
fn explicit_creation_waits_and_never_moves_the_allocator_back() {
    let db = waiting(LONG);
    let mut owner = db.begin().unwrap();
    owner.ensure_link_created(10).unwrap();
    let waiter = write_in_thread(&db, |session| session.ensure_link_created(10).map(|_| ()));
    assert!(!waiter.is_finished());
    // The allocator moves on while the other session waits.
    let mut other = db.begin().unwrap();
    other.ensure_link_created(20).unwrap();
    other.commit().unwrap();
    owner.rollback();
    waiter.join().unwrap().unwrap();
    let mut next = db.begin().unwrap();
    assert_eq!(next.create_link(1, 1).unwrap(), 21);
}

#[test]
fn a_group_commit_wakes_waiters() {
    let db = waiting(LONG);
    let mut owner = db.begin().unwrap();
    owner.set_name(1, Some("grouped".into())).unwrap();
    let mut waiter = db.begin().unwrap();
    let handle = thread::spawn(move || waiter.set_name(1, None));
    thread::sleep(Duration::from_millis(50));
    assert!(!handle.is_finished());
    let group = db.commit_group(vec![owner]).unwrap();
    assert_eq!(group.len(), 1);
    let refused = handle.join().unwrap().unwrap_err().to_string();
    assert!(refused.contains("conflict"), "{refused}");
}

#[test]
fn lock_waiting_requires_pessimistic_concurrency() {
    for concurrency in [
        ConcurrencyControl::Optimistic,
        ConcurrencyControl::Exclusive,
    ] {
        let refused = Engine::<u32, _>::open(
            MemoryLog::default(),
            EngineOptions {
                concurrency,
                lock_wait: Some(LONG),
                ..Default::default()
            },
        );
        assert!(refused.err().unwrap().to_string().contains("pessimistic"));
    }
}

#[test]
fn a_caller_that_cannot_block_learns_who_refused_it_once() {
    let db = Engine::<u32, _>::open(
        MemoryLog::default(),
        EngineOptions {
            concurrency: ConcurrencyControl::Pessimistic,
            isolation: IsolationLevel::Snapshot,
            ..Default::default()
        },
    )
    .unwrap();
    let mut seed = db.begin().unwrap();
    seed.ensure_link_created(1).unwrap();
    seed.commit().unwrap();
    let mut owner = db.begin().unwrap();
    let mut other = db.begin().unwrap();
    owner.set_name(1, Some("owned".into())).unwrap();
    assert!(other.set_name(1, None).is_err());
    assert_eq!(other.take_write_blocker(), owner.write_owner());
    assert_eq!(other.take_write_blocker(), None);
    assert_ne!(other.write_owner(), owner.write_owner());
    let optimistic =
        Engine::<u32, _>::open(MemoryLog::default(), EngineOptions::default()).unwrap();
    let mut session = optimistic.begin().unwrap();
    assert_eq!(session.write_owner(), None);
    assert_eq!(session.take_write_blocker(), None);
}

#[test]
fn a_reserved_creation_waits_for_an_explicit_owner_of_that_address() {
    let leaf = |id: &str| LinoLink {
        id: Some(id.into()),
        values: None,
    };
    let group = |values| LinoLink {
        id: None,
        values: Some(values),
    };
    let db = waiting(LONG);
    let mut waiter = db.begin().unwrap();
    let reserved = waiter
        .reserve_query_allocations(
            &QueryProcessor::new(false),
            &group(vec![]),
            &group(vec![group(vec![leaf("1"), leaf("1")])]),
        )
        .unwrap()
        .unwrap()
        .into_iter()
        .max()
        .unwrap();
    let mut owner = db.begin().unwrap();
    owner.ensure_link_created(reserved).unwrap();
    let handle = thread::spawn(move || waiter.create_link(1, 1));
    thread::sleep(Duration::from_millis(50));
    assert!(!handle.is_finished());
    owner.rollback();
    assert_eq!(handle.join().unwrap().unwrap(), reserved);
}

struct FlushFault {
    inner: MemoryLog,
    fail: Arc<AtomicBool>,
}
impl link_cli::TransitionLogStore for FlushFault {
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.inner.entries.clone())
    }
    fn append_log_entry(&mut self, record: &str) -> Result<(), LinkError> {
        self.inner.entries.push(record.into());
        Ok(())
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(LinkError::Transaction("flush".into()));
        }
        Ok(())
    }
}

#[test]
fn an_unknown_journal_outcome_ends_a_wait_with_the_reopen_error() {
    for grouped in [false, true] {
        let fail = Arc::new(AtomicBool::new(false));
        let db = Engine::<u32, _>::open(
            FlushFault {
                inner: MemoryLog::default(),
                fail: fail.clone(),
            },
            EngineOptions {
                concurrency: ConcurrencyControl::Pessimistic,
                isolation: IsolationLevel::Snapshot,
                lock_wait: Some(LONG),
                ..Default::default()
            },
        )
        .unwrap();
        let mut seed = db.begin().unwrap();
        seed.ensure_link_created(1).unwrap();
        seed.commit().unwrap();

        let mut owner = db.begin().unwrap();
        owner.set_name(1, Some("private".into())).unwrap();
        let waiter_db = db.clone();
        let (started, ready) = mpsc::channel();
        let waiter = thread::spawn(move || {
            let mut session = waiter_db.begin().unwrap();
            started.send(()).unwrap();
            session.set_name(1, Some("next".into()))
        });
        ready.recv().unwrap();
        thread::sleep(Duration::from_millis(50));

        fail.store(true, Ordering::SeqCst);
        let begun = Instant::now();
        if grouped {
            db.commit_group(vec![owner]).unwrap();
        } else {
            assert!(owner.commit().is_err());
        }
        let refused = waiter.join().unwrap().unwrap_err().to_string();
        assert!(refused.contains("reopen"), "{refused}");
        assert!(begun.elapsed() < LONG, "the waiter stayed until its limit");
    }
}
