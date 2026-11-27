use super::*;

#[test]
fn retention_policy_parses_specs() {
    assert!(matches!(
        LogRetentionPolicy::parse("infinite").unwrap(),
        LogRetentionPolicy::Infinite
    ));
    assert!(matches!(
        LogRetentionPolicy::parse("sized:1000").unwrap(),
        LogRetentionPolicy::Sized {
            max_transitions: 1000
        }
    ));
    assert!(matches!(
        LogRetentionPolicy::parse("chunked:500:/tmp/x").unwrap(),
        LogRetentionPolicy::Chunked { chunk_size: 500, archive_directory }
            if archive_directory == Path::new("/tmp/x")
    ));
    assert!(LogRetentionPolicy::parse("garbage").is_err());
}

#[test]
fn transition_round_trips_through_serialize() {
    let t = Transition {
        transaction_id: 0xabcdef1234567890u128,
        sequence: 42,
        timestamp_ms: 1234567890,
        kind: TransitionKind::Update,
        before: DoubletLink::new(1, 2, 3),
        after: DoubletLink::new(1, 4, 5),
    };
    let parsed = Transition::try_parse(&t.serialize()).unwrap();
    assert_eq!(t, parsed);
}

#[test]
fn wide_transition_is_rejected_by_a_narrow_address_type() {
    let wide = GenericTransition::<u64> {
        transaction_id: 7,
        sequence: 1,
        timestamp_ms: 0,
        kind: TransitionKind::Create,
        before: GenericDoubletLink::empty(),
        after: GenericDoubletLink::new(u32::MAX as u64 + 1, 0, 0),
    };
    assert!(matches!(
        GenericTransition::<u32>::parse(&wide.serialize()),
        Err(LinkError::AddressOutOfRange(_))
    ));
    assert!(GenericTransition::<u64>::parse(&wide.serialize()).is_ok());
}
