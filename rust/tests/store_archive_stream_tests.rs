//! Finite stream failures must not become successful or partially parsed imports.
use link_cli::{protocol::import_store, NamedTypeLinks};
use std::io::{self, Read, Write};
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/storage.rs"]
mod test_store;
#[cfg(target_arch = "wasm32")]
use crate::test_store;
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/store_archive.rs"]
mod archive;
#[cfg(target_arch = "wasm32")]
use crate::archive_fixture as archive;
use archive::{bytes, export, fill, links, names, ARCHIVE};

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
    fail_at: usize,
    chunk: usize,
}
impl Read for Reader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.position == self.fail_at {
            return Err(io::Error::other("archive read fault"));
        }
        let size = buffer
            .len()
            .min(self.chunk)
            .min(self.bytes.len() - self.position)
            .min(self.fail_at.saturating_sub(self.position));
        buffer[..size].copy_from_slice(&self.bytes[self.position..self.position + size]);
        self.position += size;
        Ok(size)
    }
}

struct Writer {
    bytes: Vec<u8>,
    fail_at: usize,
}
impl Write for Writer {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.bytes.len() == self.fail_at {
            return Err(io::Error::other("archive write fault"));
        }
        // Real streams may accept only one byte from a longer write.
        self.bytes.push(buffer[0]);
        Ok(1)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn following_import(store: &mut impl NamedTypeLinks, archive: &[u8]) {
    import_store(store, &mut &archive[..]).unwrap();
    assert_eq!(export(store), archive);
    assert!(!store.exists(2));
    let following = store.try_create(0, 0).unwrap();
    store.set_name(following, "following 雪=\0").unwrap();
    store.save().unwrap();
    assert_eq!(
        store.get_name(following).unwrap().as_deref(),
        Some("following 雪=\0")
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn read_errors_at_every_archive_boundary_preserve_existing_rows_and_names() {
    let (_files, mut target) = test_store::named().unwrap();
    let seed = target.try_create(1, 1).unwrap();
    target.set_name(seed, "keep 雪").unwrap();
    let before = (links(&mut target), names(&mut target));
    let archive = bytes(ARCHIVE);
    assert_eq!(archive.len(), 30);
    // Include the final EOF check after both complete packets.
    for fail_at in 0..=archive.len() {
        let mut input = Reader {
            bytes: &archive,
            position: 0,
            fail_at,
            chunk: 3,
        };
        let error = import_store(&mut target, &mut input).unwrap_err();
        assert!(
            error.to_string().contains("archive read fault"),
            "{fail_at}: {error}"
        );
        assert_eq!(
            (links(&mut target), names(&mut target)),
            before,
            "{fail_at}"
        );
    }
    following_import(&mut target, &archive);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_truncated_prefix_and_trailing_byte_preserves_existing_rows_and_names() {
    let (_files, mut target) = test_store::named().unwrap();
    let seed = target.try_create(1, 1).unwrap();
    target.set_name(seed, "keep 雪").unwrap();
    let before = (links(&mut target), names(&mut target));
    let archive = bytes(ARCHIVE);
    for end in 0..archive.len() {
        assert!(
            import_store(&mut target, &mut &archive[..end]).is_err(),
            "{end}"
        );
        assert_eq!((links(&mut target), names(&mut target)), before, "{end}");
    }
    for byte in [0, 0x10, 0xff] {
        let mut extra = archive.clone();
        extra.push(byte);
        assert!(
            import_store(&mut target, &mut &extra[..]).is_err(),
            "{byte}"
        );
        assert_eq!((links(&mut target), names(&mut target)), before, "{byte}");
    }
    following_import(&mut target, &archive);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn partial_export_errors_preserve_source_and_allow_a_complete_retry() {
    let (_files, mut source) = test_store::named().unwrap();
    fill(&mut source);
    let before = (links(&mut source), names(&mut source));
    let archive = bytes(ARCHIVE);
    for fail_at in 0..archive.len() {
        let mut output = Writer {
            bytes: Vec::new(),
            fail_at,
        };
        let error = link_cli::protocol::export_store(&mut source, &mut output).unwrap_err();
        assert!(
            error.to_string().contains("archive write fault"),
            "{fail_at}: {error}"
        );
        assert_eq!(output.bytes, archive[..fail_at], "{fail_at}");
        assert_eq!(
            (links(&mut source), names(&mut source)),
            before,
            "{fail_at}"
        );
    }
    let mut output = Writer {
        bytes: Vec::new(),
        fail_at: usize::MAX,
    };
    link_cli::protocol::export_store(&mut source, &mut output).unwrap();
    assert_eq!(output.bytes, archive);
    assert_eq!((links(&mut source), names(&mut source)), before);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn single_byte_reads_preserve_archive_bytes_and_repeat_imports() {
    let (_files, mut target) = test_store::named().unwrap();
    let archive = bytes(ARCHIVE);
    for _ in 0..2 {
        let mut input = Reader {
            bytes: &archive,
            position: 0,
            fail_at: usize::MAX,
            chunk: 1,
        };
        import_store(&mut target, &mut input).unwrap();
        assert_eq!(export(&mut target), archive);
        assert!(!target.exists(2));
        assert_eq!(target.get_name(1).unwrap().as_deref(), Some("a"));
        assert_eq!(target.get_name(3).unwrap().as_deref(), Some("é"));
        assert_eq!(target.get_name(4).unwrap().as_deref(), Some("ab"));
    }
}
