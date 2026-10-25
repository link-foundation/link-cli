// Run in a child with finite memory/CPU/wall limits; malformed graph has one node.
use link_cli::{LinkStorage, UnicodeStringStorage};

fn main() {
    let mut links = LinkStorage::in_memory(false);
    let mut storage = UnicodeStringStorage::new(&mut links).unwrap();
    let sequence_type = storage.unicode_sequence_type();
    let string_type = storage.string_type();
    let root = storage.links_mut().create(0, 0);
    storage.links_mut().update(root, root, root).unwrap();
    let sequence = storage.links_mut().get_or_create(root, sequence_type);
    let string = storage.links_mut().get_or_create(string_type, sequence);
    eprintln!("one-node cyclic Unicode graph prepared");
    assert!(storage.get_string(string).is_err());
    eprintln!("cyclic Unicode string rejected without exhausting resources");
}
