//! Evidence for issue #98: a file-mapped `doublets::unit::Store` built
//! straight on `platform-mem`'s `FileMapped` does **not** persist links
//! across store lifetimes — reopening zeroes the database.
//!
//! No `link-cli` code is involved: this reproduces against upstream
//! `doublets` alone. See `../README.md` §4.2 for the analysis and
//! `rust/src/storage/file_mem.rs` for the `PersistentFileMapped`
//! workaround.
//!
//! Run it with:
//!
//! ```text
//! cp docs/case-studies/issue-98/evidence/doublets_persistence.rs rust/examples/
//! cargo run --manifest-path rust/Cargo.toml --example doublets_persistence
//! rm rust/examples/doublets_persistence.rs
//! ```
//!
//! Observed with `doublets` 0.4.0:
//!
//! ```text
//! wrote a=1 b=2 count=2
//! file size = 33554432
//! reopened count=0
//! link 1 = None
//! link 2 = None
//! ```
use doublets::mem::FileMapped;
use doublets::unit::{LinkPart, Store};
use doublets::Doublets;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or("/tmp/exp.doublets".to_string());
    let _ = std::fs::remove_file(&path);
    {
        let mem = FileMapped::<LinkPart<u32>>::from_path(&path)?;
        let mut store = Store::<u32, _>::new(mem)?;
        let a = store.create_link(0, 0)?;
        let b = store.create_link(a, a)?;
        println!("wrote a={a} b={b} count={}", store.count());
    }
    println!("file size = {}", std::fs::metadata(&path)?.len());
    {
        let mem = FileMapped::<LinkPart<u32>>::from_path(&path)?;
        let store = Store::<u32, _>::new(mem)?;
        println!("reopened count={}", store.count());
        println!("link 1 = {:?}", store.get_link(1));
        println!("link 2 = {:?}", store.get_link(2));
    }
    Ok(())
}
