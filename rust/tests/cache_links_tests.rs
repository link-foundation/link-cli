//! A store kept as a cache: what comes due goes, and capacity evicts.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{bail, Result};
use link_cli::link_storage::ChangeObserver;
use link_cli::protocol::{error_message, query_document, LinksSessions};
use link_cli::{CacheLinks, CacheOptions, Link, LinkStorage, NamedTypeLinks};

fn cache_of<L: NamedTypeLinks>(links: L, options: CacheOptions) -> (CacheLinks<L>, Arc<AtomicU64>) {
    let time = Arc::new(AtomicU64::new(1000));
    let clock = time.clone();
    let cache = CacheLinks::with_clock(links, options, move || clock.load(Ordering::SeqCst));
    (cache, time)
}

fn cache(options: CacheOptions) -> (CacheLinks<LinkStorage>, Arc<AtomicU64>) {
    cache_of(LinkStorage::in_memory(false), options)
}

fn holding(max_links: usize) -> CacheLinks<LinkStorage> {
    cache(CacheOptions {
        max_links: Some(max_links),
        time_to_live: None,
    })
    .0
}

fn addresses(cache: &mut CacheLinks<LinkStorage>) -> BTreeSet<u32> {
    cache.all_links().iter().map(|link| link.index).collect()
}

fn set<const N: usize>(addresses: [u32; N]) -> BTreeSet<u32> {
    addresses.into_iter().collect()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_cache_with_no_options_is_the_store_below() {
    let (mut cache, time) = cache(CacheOptions::default());
    let link = cache.create(1, 1);
    time.store(u64::MAX, Ordering::SeqCst);
    assert!(cache.exists(link));
    assert_eq!(cache.time_to_live(link).unwrap(), None);
    assert_eq!(cache.purge_expired().unwrap(), 0);
    assert_eq!(cache.links().all().len(), 1);
    assert_eq!(cache.into_inner().all().len(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn the_system_clock_reads_now_so_a_long_deadline_has_not_passed() {
    let mut cache = CacheLinks::new(LinkStorage::in_memory(false), CacheOptions::default());
    let link = cache.create(1, 1);
    assert!(cache.expire(link, 3_600_000).unwrap());
    assert!(cache.time_to_live(link).unwrap().unwrap() > 3_000_000);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_link_goes_at_its_deadline_and_not_before() {
    let (mut cache, time) = cache(CacheOptions {
        max_links: None,
        time_to_live: Some(100),
    });
    let link = cache.create(1, 1);
    assert_eq!(cache.time_to_live(link).unwrap(), Some(100));

    time.store(1099, Ordering::SeqCst);
    assert!(cache.exists(link));
    assert_eq!(cache.time_to_live(link).unwrap(), Some(1));

    time.store(1100, Ordering::SeqCst);
    assert!(!cache.exists(link));
    assert_eq!(cache.time_to_live(link).unwrap(), None);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_read_and_write_sees_what_came_due_as_gone() {
    type Check = fn(&mut CacheLinks<LinkStorage>, u32);
    let checks: [(&str, Check); 11] = [
        ("get_link", |c, l| assert_eq!(c.get_link(l), None)),
        ("exists", |c, l| assert!(!c.exists(l))),
        ("reference_exists", |c, l| assert!(!c.reference_exists(l))),
        ("all_links", |c, _| assert!(c.all_links().is_empty())),
        ("query_links", |c, l| {
            assert!(c.query_links(Some(l), None, None).is_empty())
        }),
        ("search", |c, _| assert_eq!(c.search(1, 1), None)),
        ("get_name", |c, l| assert_eq!(c.get_name(l).unwrap(), None)),
        ("get_by_name", |c, _| {
            assert_eq!(c.get_by_name("a").unwrap(), None)
        }),
        ("update", |c, l| assert!(c.update(l, 1, 2).is_err())),
        ("delete", |c, l| assert!(c.delete(l).is_err())),
        ("save", |c, _| {
            c.save().unwrap();
            assert!(c.links().all().is_empty())
        }),
    ];
    for (name, check) in checks {
        let (mut cache, time) = cache(CacheOptions {
            max_links: None,
            time_to_live: Some(5),
        });
        let link = cache.create(1, 1);
        cache.set_name(link, "a").unwrap();
        time.fetch_add(5, Ordering::SeqCst);
        check(&mut cache, link);
        assert!(cache.links().all().is_empty(), "{name}");
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_store_that_composes_answers_through_the_cache() {
    let (mut cache, time) = cache(CacheOptions {
        max_links: None,
        time_to_live: Some(5),
    });
    assert!(cache.query_limits().is_none());
    let link = cache.create(1, 1);
    assert_eq!(cache.claims(link), None);
    time.fetch_add(5, Ordering::SeqCst);
    assert_eq!(cache.claims(link), None);
    assert!(cache.links().all().is_empty());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_name_is_the_store_s_to_keep_and_goes_with_the_link() {
    let (mut cache, time) = cache(CacheOptions::default());
    let link = cache.create(1, 1);
    cache.set_name(link, "a").unwrap();
    assert_eq!(cache.get_name(link).unwrap().as_deref(), Some("a"));
    assert_eq!(cache.get_by_name("a").unwrap(), Some(link));
    cache.remove_name(link).unwrap();
    assert_eq!(cache.get_by_name("a").unwrap(), None);

    cache.set_name(link, "b").unwrap();
    assert!(cache.expire(link, 1).unwrap());
    time.fetch_add(1, Ordering::SeqCst);
    assert_eq!(cache.get_by_name("b").unwrap(), None);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_deadline_can_be_replaced_taken_away_and_refused_for_what_is_absent() {
    let (mut cache, time) = cache(CacheOptions::default());
    let link = cache.create(1, 1);

    assert!(!cache.persist(link).unwrap());
    assert!(!cache.persist(99).unwrap());
    assert!(!cache.expire(99, 1).unwrap());
    assert_eq!(cache.time_to_live(99).unwrap(), None);

    assert!(cache.expire(link, 10).unwrap());
    assert!(cache.expire(link, 20).unwrap());
    time.fetch_add(10, Ordering::SeqCst);
    assert_eq!(cache.time_to_live(link).unwrap(), Some(10));

    assert!(cache.persist(link).unwrap());
    time.fetch_add(100, Ordering::SeqCst);
    assert!(cache.exists(link));
    assert_eq!(cache.time_to_live(link).unwrap(), None);

    // The longest time there is does not wrap around to a past deadline.
    assert!(cache.expire(link, u64::MAX).unwrap());
    assert!(cache.exists(link));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_expiry_of_zero_deletes_at_once() {
    let (mut cache, _) = cache(CacheOptions::default());
    let link = cache.create(1, 1);
    assert!(cache.expire(link, 0).unwrap());
    assert!(cache.links().all().is_empty());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn what_comes_due_takes_along_what_is_built_on_it_and_counts_it() {
    let (mut cache, time) = cache(CacheOptions::default());
    let base = cache.create(1, 1);
    let built = cache.create(base, base);
    let other = cache.create(3, 3);

    assert!(cache.expire(base, 5).unwrap());
    assert!(cache.expire(built, 50).unwrap());
    time.fetch_add(5, Ordering::SeqCst);
    assert_eq!(cache.purge_expired().unwrap(), 2);
    assert_eq!(addresses(&mut cache), set([other]));

    // The deadline of what went along went too.
    time.fetch_add(100, Ordering::SeqCst);
    assert_eq!(cache.purge_expired().unwrap(), 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn capacity_evicts_the_least_recently_used_link() {
    let mut cache = holding(2);
    let first = cache.create(1, 1);
    let second = cache.create(2, 2);
    let third = cache.create(3, 3);
    assert_eq!(addresses(&mut cache), set([second, third]));

    // Reading a link and finding it both count as using it, a scan does not.
    assert!(cache.get_link(second).is_some());
    let fourth = cache.create(4, 4);
    assert_eq!(addresses(&mut cache), set([second, fourth]));

    assert_eq!(cache.search(2, 2), Some(second));
    assert!(cache.exists(fourth));
    let fifth = cache.create(5, 5);
    assert_eq!(addresses(&mut cache), set([second, fifth]));
    assert_ne!(first, 0);

    // Nothing found and nothing there are no use.
    assert_eq!(cache.search(9, 9), None);
    assert_eq!(cache.get_link(99), None);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn updating_a_link_counts_as_using_it() {
    let mut cache = holding(2);
    let first = cache.create(1, 1);
    let second = cache.create(2, 2);
    cache.update(first, 7, 7).unwrap();
    let third = cache.create(3, 3);
    assert_eq!(addresses(&mut cache), set([first, third]));
    assert_ne!(second, 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_link_in_use_is_not_evicted_and_a_full_cache_refuses() {
    let mut cache = holding(2);
    let base = cache.create(1, 1);
    let built = cache.create(base, base);

    // Only `built` can go, and it does, because nothing uses it.
    let other = cache.create(3, 3);
    assert_eq!(addresses(&mut cache), set([base, other]));
    assert_ne!(built, 0);

    // Once used again, `base` holds and `other` is the one to go.
    let again = cache.create(base, 9);
    assert_eq!(addresses(&mut cache), set([base, again]));

    // The link a create refers to is kept for it, so nothing is left to evict.
    let refused = cache.try_create(again, base).unwrap_err().to_string();
    assert!(refused.contains("holds 2 links"), "{refused}");
    assert_eq!(addresses(&mut cache), set([base, again]));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_link_freed_of_its_users_can_be_evicted_again() {
    let mut cache = holding(2);
    let base = cache.create(1, 1);
    let built = cache.create(base, base);
    cache.delete(built).unwrap();
    let other = cache.create(3, 3);
    let last = cache.create(4, 4);
    assert_eq!(addresses(&mut cache), set([other, last]));
    assert_ne!(base, 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn updating_moves_what_a_link_uses() {
    let mut cache = holding(3);
    let old = cache.create(1, 1);
    let new = cache.create(2, 2);
    let user = cache.create(old, old);
    cache.update(user, new, 6).unwrap();

    // `old` is free now and the oldest, `new` is held.
    let other = cache.create(4, 4);
    assert_eq!(addresses(&mut cache), set([new, user, other]));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_link_two_others_use_is_held_until_both_are_gone() {
    let mut cache = holding(3);
    let base = cache.create(1, 1);
    let first = cache.create(base, base);
    let second = cache.create(base, 5);
    cache.delete(first).unwrap();
    cache.delete(second).unwrap();
    let a = cache.create(7, 7);
    let b = cache.create(8, 8);
    let c = cache.create(9, 9);
    assert_eq!(addresses(&mut cache), set([a, b, c]));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_link_naming_itself_is_no_use_of_itself() {
    let mut cache = holding(1);
    let link = cache.create(0, 0);
    cache.update(link, link, link).unwrap();
    let next = cache.create(9, 9);
    assert_eq!(addresses(&mut cache), set([next]));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_cache_holding_nothing_refuses_everything() {
    let mut cache = holding(0);
    assert!(cache.try_create(1, 1).is_err());
    assert!(cache.try_ensure_created(5).is_err());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn what_cannot_answer_a_refusal_panics_with_it() {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let mut cache = holding(0);
    assert!(catch_unwind(AssertUnwindSafe(|| cache.create(1, 1))).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| cache.ensure_created(5))).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| cache.get_or_create(1, 1))).is_err());

    let (mut cache, time) = cache_of(
        Below {
            inner: LinkStorage::in_memory(false),
            refuses: true,
        },
        CacheOptions::default(),
    );
    let link = cache.try_create(1, 1).unwrap();
    assert!(cache.expire(link, 5).unwrap());
    time.fetch_add(5, Ordering::SeqCst);
    assert!(catch_unwind(AssertUnwindSafe(|| cache.exists(link))).is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn links_already_there_are_the_first_to_go_and_have_no_deadline() {
    let mut links = LinkStorage::in_memory(false);
    let first = links.create(1, 1);
    let second = links.create(first, first);
    let third = links.create(3, 3);
    let (mut cache, time) = cache_of(
        links,
        CacheOptions {
            max_links: Some(3),
            time_to_live: Some(5),
        },
    );
    assert_eq!(cache.time_to_live(first).unwrap(), None);
    let new = cache.create(4, 4);
    assert_eq!(addresses(&mut cache), set([first, third, new]));
    assert_ne!(second, 0);

    // Only the new link has a deadline.
    time.fetch_add(5, Ordering::SeqCst);
    assert_eq!(addresses(&mut cache), set([first, third]));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn creating_at_an_address_is_tracked_and_makes_room() {
    let mut cache = holding(2);
    let first = cache.create(1, 1);
    assert_eq!(cache.ensure_created(first), first);
    assert!(cache.try_ensure_created(0).is_err());
    assert_eq!(cache.try_ensure_created(10).unwrap(), 10);
    assert_eq!(addresses(&mut cache), set([first, 10]));

    assert_eq!(cache.ensure_created(11), 11);
    assert_eq!(addresses(&mut cache), set([10, 11]));
    assert!(cache.expire(11, 1000).unwrap());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn get_or_create_finds_before_it_makes_room() {
    let mut cache = holding(1);
    let link = cache.get_or_create(1, 1);
    assert_eq!(cache.get_or_create(1, 1), link);
    assert_eq!(cache.links().all().len(), 1);
    let other = cache.get_or_create(2, 2);
    assert_eq!(addresses(&mut cache), set([other]));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn asking_what_comes_next_evicts_nothing() {
    let mut cache = holding(1);
    let link = cache.create(1, 1);
    assert_eq!(cache.next_created_addresses(2).unwrap().len(), 2);
    assert_eq!(addresses(&mut cache), set([link]));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_update_that_merges_into_a_duplicate_is_followed() {
    let mut cache = holding(3);
    let keep = cache.create(1, 1);
    let merged = cache.create(2, 2);
    let mut changes = Vec::new();
    cache
        .update_observed(merged, 1, 1, &mut |before, after| {
            changes.push((before, after))
        })
        .unwrap();
    assert!(changes.iter().any(|(_, after)| after.is_null()));
    assert_eq!(addresses(&mut cache), set([keep]));
    let (a, b) = (cache.create(3, 3), cache.create(4, 4));
    assert_eq!(addresses(&mut cache), set([keep, a, b]));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_delete_reports_to_the_caller_as_well() {
    let mut cache = holding(3);
    let base = cache.create(1, 1);
    let built = cache.create(base, base);
    let mut deleted = Vec::new();
    cache
        .delete_observed(base, &mut |before, after| {
            if after.is_null() {
                deleted.push(before.index);
            }
        })
        .unwrap();
    deleted.sort_unstable();
    assert_eq!(deleted, vec![base, built]);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_session_and_a_query_work_over_a_cache() {
    let (cache, time) = cache(CacheOptions {
        max_links: Some(2),
        time_to_live: Some(10),
    });
    let mut sessions = LinksSessions::new(cache);
    let session = sessions.open(None);

    let reply = sessions.execute(session, &query_document("() ((1: 1 1) (2: 2 2))"));
    assert_eq!(error_message(&reply), None);
    assert_eq!(sessions.store_mut().all_links().len(), 2);

    // A third link evicts the oldest one.
    let reply = sessions.execute(session, &query_document("() ((3: 3 3))"));
    assert_eq!(error_message(&reply), None);
    assert_eq!(sessions.store_mut().all_links().len(), 2);

    time.fetch_add(10, Ordering::SeqCst);
    assert!(sessions.store_mut().all_links().is_empty());
}

/// Fails to delete when told to, and makes a link of its own for each name.
struct Below {
    inner: LinkStorage,
    refuses: bool,
}

impl NamedTypeLinks for Below {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        self.inner.create(source, target)
    }
    fn ensure_created(&mut self, id: u32) -> u32 {
        self.inner.ensure_created(id)
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
        self.inner.update_observed(id, source, target, changed)
    }
    fn delete_observed(&mut self, id: u32, changed: ChangeObserver<'_>) -> Result<Link> {
        if self.refuses {
            bail!("injected fault");
        }
        self.inner.delete_observed(id, changed)
    }
    fn all_links(&mut self) -> Vec<Link> {
        NamedTypeLinks::all_links(&mut self.inner)
    }
    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        self.inner.search(source, target)
    }
    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        self.inner.get_or_create(source, target)
    }
    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        NamedTypeLinks::get_name(&mut self.inner, id)
    }
    fn set_name(&mut self, id: u32, _: &str) -> Result<u32> {
        // What is built on a link the cache did not see made.
        self.inner.create(id, id);
        Ok(self.inner.create(100, 100))
    }
    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        NamedTypeLinks::get_by_name(&mut self.inner, name)
    }
    fn remove_name(&mut self, id: u32) -> Result<()> {
        NamedTypeLinks::remove_name(&mut self.inner, id)
    }
    fn save(&mut self) -> Result<()> {
        self.inner.save()
    }
}

fn below() -> Below {
    Below {
        inner: LinkStorage::in_memory(false),
        refuses: false,
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_store_that_cannot_delete_what_came_due_fails_the_call() {
    let (mut cache, time) = cache_of(below(), CacheOptions::default());
    let link = cache.try_create(1, 1).unwrap();
    assert!(cache.expire(link, 1).unwrap());
    time.fetch_add(1, Ordering::SeqCst);
    assert_eq!(cache.purge_expired().unwrap(), 1);

    let mut refusing = cache_of(
        Below {
            inner: LinkStorage::in_memory(false),
            refuses: true,
        },
        CacheOptions::default(),
    );
    let other = refusing.0.try_create(1, 1).unwrap();
    assert!(refusing.0.expire(other, 5).unwrap());
    refusing.1.fetch_add(5, Ordering::SeqCst);
    assert!(refusing.0.try_create(2, 2).is_err());
    assert!(refusing.0.purge_expired().is_err());
    assert!(refusing.0.time_to_live(other).is_err());
    assert!(refusing.0.persist(other).is_err());
    assert!(refusing.0.expire(other, 1).is_err());
    assert!(refusing.0.update(other, 1, 2).is_err());
    assert!(refusing.0.delete(other).is_err());
    assert!(refusing.0.get_name(other).is_err());
    assert!(refusing.0.set_name(other, "a").is_err());
    assert!(refusing.0.get_by_name("a").is_err());
    assert!(refusing.0.remove_name(other).is_err());
    assert!(refusing.0.save().is_err());
    assert!(refusing.0.next_created_addresses(1).is_err());
    assert!(refusing.0.try_ensure_created(9).is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_store_that_cannot_evict_refuses_the_create_and_keeps_the_link() {
    let (mut cache, _) = cache_of(
        below(),
        CacheOptions {
            max_links: Some(1),
            time_to_live: None,
        },
    );
    let link = cache.try_create(1, 1).unwrap();
    let mut refusing = cache_of(
        Below {
            inner: LinkStorage::in_memory(false),
            refuses: true,
        },
        CacheOptions {
            max_links: Some(1),
            time_to_live: None,
        },
    )
    .0;
    let kept = refusing.try_create(1, 1).unwrap();
    assert!(refusing.try_create(2, 2).is_err());
    assert!(refusing.exists(kept));
    assert!(cache.exists(link));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn links_the_store_below_made_itself_are_read_and_not_counted() {
    let (mut cache, _) = cache_of(
        below(),
        CacheOptions {
            max_links: Some(5),
            time_to_live: None,
        },
    );
    let link = cache.try_create(1, 1).unwrap();
    let made = cache.set_name(link, "a").unwrap();
    assert!(cache.get_link(made).is_some());
    assert_eq!(cache.search(100, 100), Some(made));
    assert!(!cache.expire(made, 5).unwrap());
    cache.delete(made).unwrap();

    // Taking `link` down reports the link built on it, which was never counted.
    cache.delete(link).unwrap();
    assert!(cache.all_links().is_empty());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_link_may_refer_to_an_address_nothing_holds_yet() {
    let mut cache = holding(2);
    let user = cache.create(5, 5);
    // The address arrives with its usage already counted, so it is held from the start.
    assert_eq!(cache.ensure_created(5), 5);
    cache.delete(user).unwrap();
    let other = cache.create(7, 7);
    let last = cache.create(8, 8);
    assert_eq!(addresses(&mut cache), set([other, last]));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn the_documented_example_works() -> Result<()> {
    let options = CacheOptions {
        max_links: Some(1000),
        time_to_live: Some(60_000),
    };
    let mut cache = CacheLinks::new(LinkStorage::in_memory(false), options);
    let link = cache.create(1, 1);
    assert!(cache.time_to_live(link)?.is_some_and(|left| left <= 60_000));
    assert!(cache.persist(link)?);
    assert_eq!(cache.time_to_live(link)?, None);
    assert!(cache.expire(link, 5_000)?);
    assert!(cache.time_to_live(link)?.is_some());
    Ok(())
}
