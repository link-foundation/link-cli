//! Links every WebAssembly artifact of this crate with an 8 MiB stack.
//!
//! The LiNo parser is recursive. On native targets `link-cli` parses imports
//! on a scoped thread with an 8 MiB stack, but WebAssembly has no threads, so
//! the parse runs on the module's shadow stack, which `wasm-ld` sizes at 1 MiB
//! by default. That is too small for the documented `--in-max-depth` limit of
//! 128 levels in debug and coverage builds (issue #200). A build script
//! applies the size to the cdylib and to the test executables alike, whatever
//! the working directory or `RUSTFLAGS` of the build.

const WASM_STACK_SIZE: usize = 8 * 1024 * 1024;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo:rustc-link-arg=-zstack-size={WASM_STACK_SIZE}");
    }
}
