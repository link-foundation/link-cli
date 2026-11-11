# Release lockfile regression

Main's automated Rust 3.0.0 release updated `rust/Cargo.lock` while leaving
`rust/wasm/Cargo.lock` at 2.1.2. The existing SQLite dependency-boundary check
then failed at `cargo tree --locked` for the WASM package.

Run the actual release-script unit tests without compiling the database:

```sh
bash experiments/issue-140/release-lockfiles/run.sh
```

The staging regression creates a miniature Git repository, bumps a package
used by a separate WASM workspace, and verifies both updated locks are staged
with the manifest, changelog and removed fragments. It failed before syncing
the dependent lockfile. The helper build uses one compiler and no debug info;
the first local compilation peaked at 284 MiB RSS. Generated Cargo metadata
and build outputs are ignored. CI also runs this test through the existing
`rust/scripts/test-scripts.sh`, with the release script's full dependencies.
