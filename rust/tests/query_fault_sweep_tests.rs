//! Every query is run against a store that fails at each step in turn, and under
//! each limit in turn. Whatever fails, the processor reports it instead of
//! panicking, and the same query succeeds once nothing fails any more.

use std::cell::Cell;

use anyhow::{bail, Result};
use link_cli::link_storage::ChangeObserver;
use link_cli::query_options::QueryLimits;
use link_cli::{Link, LinkStorage, NamedTypeLinks, QueryProcessor};

struct Faulty {
    inner: LinkStorage,
    steps_left: Cell<usize>,
    limits: Option<QueryLimits>,
}

impl Faulty {
    fn step(&self) -> Result<()> {
        match self.steps_left.get() {
            0 => bail!("injected fault"),
            left => {
                self.steps_left.set(left - 1);
                Ok(())
            }
        }
    }
}

impl NamedTypeLinks for Faulty {
    fn query_limits(&self) -> Option<QueryLimits> {
        self.limits
    }
    fn create(&mut self, source: u32, target: u32) -> u32 {
        self.inner.create(source, target)
    }
    fn try_create(&mut self, source: u32, target: u32) -> Result<u32> {
        self.step()?;
        Ok(self.inner.create(source, target))
    }
    fn ensure_created(&mut self, id: u32) -> u32 {
        self.inner.ensure_created(id)
    }
    fn try_ensure_created(&mut self, id: u32) -> Result<u32> {
        self.step()?;
        Ok(self.inner.ensure_created(id))
    }
    fn get_link(&mut self, id: u32) -> Option<Link> {
        self.inner.get_link(id)
    }
    fn exists(&mut self, id: u32) -> bool {
        self.inner.exists(id)
    }
    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        changed: ChangeObserver<'_>,
    ) -> Result<Link> {
        self.step()?;
        self.inner.update_observed(id, source, target, changed)
    }
    fn delete_observed(&mut self, id: u32, changed: ChangeObserver<'_>) -> Result<Link> {
        self.step()?;
        self.inner.delete_observed(id, changed)
    }
    fn all_links(&mut self) -> Vec<Link> {
        self.inner.all_links()
    }
    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        self.inner.search(source, target)
    }
    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        self.inner.get_or_create(source, target)
    }
    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        self.step()?;
        NamedTypeLinks::get_name(&mut self.inner, id)
    }
    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        self.step()?;
        NamedTypeLinks::set_name(&mut self.inner, id, name)
    }
    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        self.step()?;
        NamedTypeLinks::get_by_name(&mut self.inner, name)
    }
    fn remove_name(&mut self, id: u32) -> Result<()> {
        self.step()?;
        NamedTypeLinks::remove_name(&mut self.inner, id)
    }
    fn save(&mut self) -> Result<()> {
        self.inner.save()
    }
}

const SEED: &str = "() ((1: 1 1) (2: 1 2) (3: 2 3) (first: 1 3) (5: 5 5) (6: 6 5))";

const QUERIES: &[&str] = &[
    // Creating, by address, by name, nested.
    "() ((9: 9 9))",
    "() ((named: named named))",
    "() ((other: 1 first))",
    "() (((1 2) (2 3)))",
    "() ((new: (1 2) (2 3)))",
    "() ((1 1))",
    "() ((1: 1 1))",
    // Reading, with and without variables.
    "((($i: $s $t)) ((($i: $s $t))))",
    "((($i: 1 $t)) (($i: 1 $t)))",
    "(((1: $s $t)) ((1: $s $t)))",
    "((($i: $s $t) ($j: $t $u)) (($i: $s $t) ($j: $t $u)))",
    "(((* * *)) ((* * *)))",
    "((*: 1 *)) ((*: 1 *))",
    "((first: 1 3)) ((first: 1 3))",
    "((nobody: 1 3)) ((nobody: 1 3))",
    // Changing what matched.
    "(((1: 1 2)) ((1: 2 1)))",
    "((($i: 1 $t)) (($i: $t 1)))",
    "(((2: 1 2)) ((2: 2 2)))",
    "(((3: 2 3)) ((1 1)))",
    "((($i: 2 $t)) ((1 $t)))",
    "(((first: 1 3)) ((first: 3 1)))",
    "(((1: 1 1)) ((1: 1 1) (other: 1 1)))",
    "(((1: 1 1) (2: 1 2)) ((1: 1 1)))",
    "(((2: 1 2)) ((7: 1 2)))",
    // Deleting.
    "(((3: 2 3)) ())",
    "((($i: 5 5)) ())",
    "(((first: 1 3)) ())",
    "((($i: 1 $t)) ())",
    "(((* * *)) ())",
    "(id: (2: 1 2)) ((2: 1 2))",
    // Names read while resolving the parts of either side.
    "((first)) ((first))",
    "(((1: first 3))) ((1: first 3))",
    "((first: first 3)) ((first: first 3))",
    "(((1: 1 1)) ((nested: (first 3) (1 first))))",
    "(((1: 1 1)) ((1: 1 1) (fresh: first first)))",
    "((($i: first $t)) (($i: $t first)))",
];

fn processor() -> QueryProcessor {
    QueryProcessor::new(false).with_auto_create_missing_references(true)
}

fn seeded(limits: Option<QueryLimits>, steps: usize) -> Result<Faulty> {
    let mut store = Faulty {
        inner: LinkStorage::in_memory(false),
        steps_left: Cell::new(usize::MAX),
        limits: None,
    };
    processor().process_query(&mut store, SEED)?;
    store.limits = limits;
    store.steps_left.set(steps);
    Ok(store)
}

#[test]
fn a_store_failing_at_any_step_is_reported_and_then_the_query_runs() -> Result<()> {
    for query in QUERIES {
        let answer = processor().process_query(&mut seeded(None, usize::MAX)?, query);
        for steps in 0..200 {
            let mut store = seeded(None, steps)?;
            match processor().process_query(&mut store, query) {
                Ok(_) => break,
                Err(error) if error.to_string().contains("injected fault") => {}
                // A query that cannot run fails the same way without the fault.
                Err(error) => {
                    assert_eq!(
                        answer.as_ref().err().map(ToString::to_string),
                        Some(error.to_string()),
                        "{query}"
                    );
                    break;
                }
            }
            assert!(steps < 199, "{query} never finished");
        }
    }
    Ok(())
}

fn limited(change: impl Fn(&mut QueryLimits, usize)) -> Result<()> {
    for query in QUERIES {
        let answer = processor().process_query(&mut seeded(None, usize::MAX)?, query);
        for limit in 0..64 {
            let mut limits = QueryLimits::causal();
            change(&mut limits, limit);
            // A query the limit admits answers exactly as it does without limits.
            if let Ok(changes) =
                processor().process_query(&mut seeded(Some(limits), usize::MAX)?, query)
            {
                let unlimited = answer.as_ref().unwrap_or_else(|error| {
                    panic!("{query} passed a limit but fails without one: {error}")
                });
                assert_eq!(&changes, unlimited, "{query}");
            }
        }
    }
    Ok(())
}

#[test]
fn a_result_limit_refuses_queries_reaching_it() -> Result<()> {
    limited(|limits, limit| limits.max_results = limit)
}

#[test]
fn a_binding_limit_refuses_queries_reaching_it() -> Result<()> {
    limited(|limits, limit| limits.max_bindings = limit)
}

#[test]
fn a_binding_size_limit_refuses_queries_reaching_it() -> Result<()> {
    limited(|limits, limit| limits.max_binding_bytes = limit)
}

#[test]
fn a_node_and_depth_limit_refuse_queries_reaching_them() -> Result<()> {
    limited(|limits, limit| limits.max_nodes = limit)?;
    limited(|limits, limit| limits.max_depth = limit)
}
