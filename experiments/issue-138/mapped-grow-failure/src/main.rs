//! Finite, quota-bounded child: inspect the upstream rejected-remap lifecycle.
use link_cli::doublets::mem::RawMem;
use link_cli::PersistentFileMapped;

// Every persisted bit pattern remains a valid integer. Drop has no resources
// to follow, but selects the public adapter's generic destructor-bearing path.
#[repr(transparent)]
#[derive(Clone, Debug)]
struct Droppable(u64);

impl Drop for Droppable {
    fn drop(&mut self) {}
}

fn generic(mode: &str, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    assert!(std::mem::needs_drop::<Droppable>());
    let mut mapped = PersistentFileMapped::<Droppable>::from_path(path)?;
    mapped.grow_filled(32, Droppable(42))?;
    let barrier = mapped.flush_handle();
    barrier.flush()?;
    if mode == "generic-within-quota" {
        mapped.grow_filled(64, Droppable(19))?;
        assert_eq!(mapped.allocated()[0].0, 42);
        barrier.flush()?;
        println!("PASS finite generic growth preserves the live prefix");
        return Ok(());
    }
    assert_eq!(mode, "generic-rejected-growth");
    let error = mapped.grow_filled(8192, Droppable(19)).unwrap_err();
    eprintln!("REJECTED generic growth: {error}");
    let error = barrier.flush().unwrap_err();
    assert!(error.to_string().contains("unfinished remap"));
    eprintln!("REJECTED stale generic flush descriptor before pointer access");
    // The parent bounds this intentional negative control and disables core
    // dumps. This safe public read after a returned error must remain valid.
    println!(
        "retained generic prefix={}",
        std::hint::black_box(mapped.allocated()[0].0)
    );
    println!("PASS retained generic mapping after rejected growth");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let mode = arguments.next().ok_or("missing mode")?;
    let path = arguments.next().ok_or("missing path")?;
    if mode.starts_with("generic-") {
        return generic(&mode, &path);
    }
    let mut mapped = PersistentFileMapped::<u64>::from_path(path)?;
    mapped.grow_filled(32, 0)?[0] = 42;
    let barrier = mapped.flush_handle();
    barrier.flush()?;
    if mode == "within-quota" {
        mapped.grow_filled(64, 0)?;
        assert_eq!(mapped.allocated()[0], 42);
        barrier.flush()?;
        println!("PASS finite growth within the file quota preserves the live prefix");
        return Ok(());
    }
    assert_eq!(mode, "rejected-growth");
    // The parent sets a 4096-byte file limit. This requests only ~66 KiB;
    // set_len must fail, with SIGXFSZ ignored so the error can be inspected.
    let error = mapped.grow_filled(8192, 0).unwrap_err();
    eprintln!(
        "REJECTED growth: {error}; retained length={}",
        mapped.allocated().len()
    );
    match barrier.flush() {
        Err(error) => {
            assert!(error.to_string().contains("unfinished remap"));
            eprintln!("REJECTED stale flush descriptor before pointer access");
        }
        Ok(()) => eprintln!("PASS retained flush descriptor after rejected growth"),
    }
    // This is safe public API usage after a returned error. Run it only in
    // the finite, core-disabled subprocess: upstream may have unmapped it.
    println!(
        "retained prefix={}",
        std::hint::black_box(mapped.allocated()[0])
    );
    println!("PASS retained mapping after rejected growth");
    Ok(())
}
