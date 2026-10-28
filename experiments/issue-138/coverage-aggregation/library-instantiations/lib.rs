pub fn supported() -> bool {
    true
}
pub fn select<T>(left: bool) -> u32 {
    let _type_size = std::mem::size_of::<T>();
    if left {
        11
    } else {
        22
    }
}
