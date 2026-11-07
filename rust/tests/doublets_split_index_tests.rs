//! Contracts for the upstream split indexes used by mapped CLI storage.
//! Keep this file independent of link-cli so the bounded experiment can run
//! the exact same assertions against the previous and current dependency.

use doublets::data::Flow;
use doublets::mem::Global;
use doublets::split::Store;
use doublets::{Doublets, Links};

fn rows(links: &impl Doublets<u32>, source: u32, target: u32) -> Vec<(u32, u32, u32)> {
    let mut result = Vec::new();
    links.each_links(&[links.constants().any, source, target], &mut |link| {
        result.push((link.index, link.source, link.target));
        Flow::Continue
    });
    result.sort_unstable();
    result
}

#[test]
fn absent_source_count_filters_the_concrete_target() {
    let mut links = Store::<u32, _, _>::new(Global::new(), Global::new()).unwrap();
    let first = links.create_link(9, 7).unwrap();
    let second = links.create_link(9, 8).unwrap();
    let any = links.constants().any;

    assert_eq!(rows(&links, 9, any), vec![(first, 9, 7), (second, 9, 8)]);
    assert_eq!(links.count_links(&[any, 9, any]), 2);
    assert_eq!(links.count_links(&[any, 9, 7]), 1);
    assert_eq!(links.count_links(&[any, 9, 8]), 1);
    assert_eq!(links.count_links(&[any, 9, 6]), 0);
}

#[test]
fn allocating_forward_source_preserves_indexed_queries_and_updates() {
    let mut links = Store::<u32, _, _>::new(Global::new(), Global::new()).unwrap();
    let first = links.create_link(2, 7).unwrap();
    assert_eq!(rows(&links, 2, 7), vec![(first, 2, 7)]);
    assert_eq!(links.create_link(2, 2).unwrap(), 2);
    let any = links.constants().any;

    assert_eq!(rows(&links, 2, any), vec![(first, 2, 7), (2, 2, 2)]);
    assert_eq!(links.count_links(&[any, 2, any]), 2);
    links.update(first, 2, 8).unwrap();
    assert_eq!(rows(&links, 2, 7), vec![]);
    assert_eq!(rows(&links, 2, 8), vec![(first, 2, 8)]);
    links.delete(first).unwrap();
    assert_eq!(rows(&links, 2, any), vec![(2, 2, 2)]);
}

#[test]
fn allocating_forward_target_preserves_indexed_queries_and_deletes() {
    let mut links = Store::<u32, _, _>::new(Global::new(), Global::new()).unwrap();
    let first = links.create_link(7, 2).unwrap();
    assert_eq!(rows(&links, 7, 2), vec![(first, 7, 2)]);
    assert_eq!(links.create_link(2, 2).unwrap(), 2);
    let any = links.constants().any;

    assert_eq!(rows(&links, any, 2), vec![(first, 7, 2), (2, 2, 2)]);
    assert_eq!(links.count_links(&[any, any, 2]), 2);
    links.delete(first).unwrap();
    assert_eq!(rows(&links, any, 2), vec![(2, 2, 2)]);
}

#[test]
fn allocating_shared_forward_endpoint_preserves_both_indexes() {
    let mut links = Store::<u32, _, _>::new(Global::new(), Global::new()).unwrap();
    let first = links.create_link(2, 2).unwrap();
    assert_eq!(rows(&links, 2, 2), vec![(first, 2, 2)]);
    assert_eq!(links.create_link(3, 3).unwrap(), 2);
    let any = links.constants().any;
    let expected = vec![(first, 2, 2)];

    assert_eq!(rows(&links, 2, any), expected);
    assert_eq!(rows(&links, any, 2), expected);
    assert_eq!(links.count_links(&[any, 2, 2]), 1);
    links.update(first, 7, 8).unwrap();
    assert_eq!(rows(&links, 2, any), vec![]);
    assert_eq!(rows(&links, any, 2), vec![]);
    assert_eq!(rows(&links, 7, 8), vec![(first, 7, 8)]);
}
