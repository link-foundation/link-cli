fn main() {
    assert!(instantiation_probe::supported());
    assert_eq!(instantiation_probe::select::<u32>(true), 11);
    assert_eq!(instantiation_probe::select::<u32>(false), 22);
    #[cfg(two_types)]
    {
        assert_eq!(instantiation_probe::select::<u64>(true), 11);
        assert_eq!(instantiation_probe::select::<u64>(false), 22);
    }
}
