// Both outcomes execute across two instantiations of one generic function.
fn select<T>(left: bool) -> u32 {
    let _type_size = std::mem::size_of::<T>();
    if left {
        11
    } else {
        22
    }
}
fn main() {
    assert_eq!(select::<u32>(true), 11);
    assert_eq!(select::<u64>(false), 22);
}
