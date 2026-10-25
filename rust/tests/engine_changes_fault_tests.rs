//! Finite independent durable images; no assumption that failed flush lost writes.
use link_cli::engine::{
    Constraint, ConstraintValidation, DeltaLog, Engine, EngineOptions, GroupCommitOutcome,
    HistoryPolicy, IsolationLevel, RevisionEncoding, StateStructure,
};
use link_cli::{LinkError, LinksStorage, TransitionLogStore};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Image {
    durable: Vec<String>,
    staged: Vec<String>,
    mode: &'static str,
    prefix: usize,
    appends: usize,
    flushes: usize,
}
struct Log(Rc<RefCell<Image>>);
impl TransitionLogStore for Log {
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.0.borrow().durable.clone())
    }
    fn append_log_entry(&mut self, record: &str) -> Result<(), LinkError> {
        let mut image = self.0.borrow_mut();
        image.appends += 1;
        let failing = image.appends == 2 && image.mode.starts_with("append");
        if !failing || image.mode == "append-after" {
            image.staged.push(record.into());
        }
        if failing {
            image.durable = image.staged[..1 + image.prefix].to_vec();
            return Err(LinkError::Transaction("independent append image".into()));
        }
        Ok(())
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        let mut image = self.0.borrow_mut();
        image.flushes += 1;
        if image.mode == "flush" {
            image.durable = image.staged[..1 + image.prefix].to_vec();
            return Err(LinkError::Transaction("independent flush image".into()));
        }
        image.durable = image.staged.clone();
        Ok(())
    }
}

fn schedule<L: TransitionLogStore>(
    log: L,
    image: Rc<RefCell<Image>>,
    structure: StateStructure,
    mode: &'static str,
    prefix: usize,
    validation: ConstraintValidation,
) {
    let options = EngineOptions {
        revision_encoding: RevisionEncoding::Changes,
        state_structure: structure,
        isolation: IsolationLevel::ReadCommitted,
        history: HistoryPolicy::All,
        constraint_validation: validation,
        constraints: vec![Constraint::UniqueNames],
        ..Default::default()
    };
    let db = Engine::<u32, _>::open(log, options.clone()).unwrap();
    let mut seed = db.begin().unwrap();
    seed.ensure_link_created(1).unwrap();
    seed.ensure_link_created(2).unwrap();
    seed.commit().unwrap();
    drop(seed);
    let pinned = db.snapshot().unwrap();
    let mut observer = db.begin().unwrap();
    {
        let mut image = image.borrow_mut();
        image.mode = mode;
        image.prefix = prefix;
        image.appends = 0;
        image.flushes = 0;
    }
    let mut first = db.begin().unwrap();
    let mut second = db.begin().unwrap();
    first.set_name(1, Some("one 雪\0".into())).unwrap();
    second.set_name(2, Some("".into())).unwrap();
    let outcomes = db.commit_group(vec![first, second]).unwrap();
    assert!(outcomes
        .iter()
        .all(|value| matches!(value, GroupCommitOutcome::Indeterminate(_))));
    assert!(pinned.get(1).unwrap().name.is_none());
    assert!(pinned.get(2).unwrap().name.is_none());
    assert_eq!(observer.get_name(1), None);
    assert_eq!(observer.get_name(2), None);
    assert!(db.snapshot().is_err());
    assert!(db.begin().is_err());
    let observed = {
        let image = image.borrow();
        (image.appends, image.flushes)
    };
    assert_eq!(observed, (2, usize::from(mode == "flush")));
    observer.rollback();
    drop(observer);
    drop(db);
    {
        let mut image = image.borrow_mut();
        image.staged = image.durable.clone();
        image.mode = "";
    }
    // Changes pass through physical delta compression exactly, so a plain
    // reopened log is also a control against hidden complete-state dependence.
    let reopened = Engine::<u32, _>::open(Log(image.clone()), options).unwrap();
    let actual = reopened.snapshot().unwrap();
    assert_eq!(actual.revision, 1 + prefix as u64);
    assert_eq!(
        actual.get(1).unwrap().name.as_deref(),
        (prefix >= 1).then_some("one 雪\0")
    );
    assert_eq!(
        actual.get(2).unwrap().name.as_deref(),
        (prefix >= 2).then_some("")
    );
    assert!(reopened
        .snapshot_at(1)
        .unwrap()
        .get(1)
        .unwrap()
        .name
        .is_none());
    let mut following = reopened.begin().unwrap();
    following.set_name(1, Some("following".into())).unwrap();
    following.commit().unwrap();
    assert!(pinned.get(1).unwrap().name.is_none());
    drop(following);
    drop(reopened);
    let again = Engine::<u32, _>::open(Log(image), EngineOptions::default()).unwrap();
    assert_eq!(
        again.snapshot().unwrap().get(1).unwrap().name.as_deref(),
        Some("following")
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn uncertain_change_groups_recover_each_possible_durable_prefix_without_publishing_tentative_rows()
{
    for structure in [StateStructure::Flat, StateStructure::Shared] {
        for delta in [false, true] {
            for (mode, max_prefix) in [("append-before", 1), ("append-after", 2), ("flush", 2)] {
                for prefix in 0..=max_prefix {
                    for validation in [ConstraintValidation::Scan, ConstraintValidation::Indexed] {
                        let image = Rc::new(RefCell::new(Image::default()));
                        let log = Log(image.clone());
                        if delta {
                            schedule(
                                DeltaLog::new(log, 128).unwrap(),
                                image,
                                structure,
                                mode,
                                prefix,
                                validation,
                            );
                        } else {
                            schedule(log, image, structure, mode, prefix, validation);
                        }
                    }
                }
            }
        }
    }
}
