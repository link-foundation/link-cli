//! A custom harness permits faulting stdout before library code runs.
#[cfg(target_os = "linux")]
fn main() {
    use link_cli::{
        NamedTypeLinks, NamedTypesDecorator, PersistentTransformationDecorator,
        PersistentTransformationKind, TriggerStore,
    };
    if let Some(path) = std::env::var_os("LINK_STDOUT_DIAGNOSTIC_PATH") {
        let path = std::path::PathBuf::from(path);
        let links = NamedTypesDecorator::new(path.join("data"), false).unwrap();
        let triggers = TriggerStore::sidecar(path.join("triggers"), false).unwrap();
        let mut store = PersistentTransformationDecorator::new(links, triggers, true);
        let query = "(((1: 1 1)) ((1: 1 2)))";
        let root = store
            .store_trigger(PersistentTransformationKind::Always, query)
            .unwrap();
        assert_ne!(root, 0);
        assert_eq!(store.triggers().unwrap().len(), 1);
        assert_eq!(store.remove_triggers(query).unwrap(), 1);
        assert!(store.triggers().unwrap().is_empty());
        store.save().unwrap();
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .env("LINK_STDOUT_DIAGNOSTIC_PATH", directory.path())
        .stdout(
            std::fs::OpenOptions::new()
                .write(true)
                .open("/dev/full")
                .unwrap(),
        )
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "stdout diagnostic child {}: {}",
        child.status,
        String::from_utf8_lossy(&child.stderr)
    );
    println!("stdout diagnostic subprocess: passed");
}

#[cfg(not(target_os = "linux"))]
fn main() {
    println!("stdout diagnostic subprocess: Linux ENOSPC fixture skipped");
}
