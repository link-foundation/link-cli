# Legacy native text storage

Rust's `LinkStorage` stores its own in-memory doublets and optional inline names
in a line-oriented replacement file. It requires no SQLite provider. The normal
`NamedTypesDecorator` Unicode names sidecar, C# mapped doublets and engine revision
logs have their own formats; this file is not an interchange format for them.

Ordinary rows retain their existing representation:

```text
(1 10 11 "article title 雪")
(2 12 13)
```

The reader consumes the three numeric fields, then preserves the complete quoted
name tail with exactly one enclosing quote pair removed. Interior/edge quotes,
spaces, Unicode and parentheses are literal. Backslashes have no escape syntax:
`"a\nb"` contains a backslash and `n`. An unquoted `~1{31}` also remains that
literal name. Empty quoted names remain distinct from unnamed rows. Legacy
unquoted single-token names, optional row delimiters, CRLF, comments, free-list
headers and ignored malformed legacy rows keep their existing behavior.

ASCII controls, including CR/LF, NUL, tabs and DEL, cannot be embedded in an
ordinary saved row name. The writer emits that row unnamed and adds versioned
name metadata after all rows. Names for absent addresses also use this metadata:

```text
(1 10 11)
# link-cli:name:1 0 ~1{}
# link-cli:name:1 1 ~1{610a62}
```

This independently specified example names address zero with the empty string
and row one with `a`, a newline, then `b`. Each name uses the existing
links-notation version-1 literal: exact UTF-8 bytes as hexadecimal inside
`~1{...}`. Addresses are unsigned decimal u32 values; signs and overflow reject.
Metadata creates no rows and does not advance allocation, including at zero or
u32::MAX. Name assignment retains one owner per name; later assignments move
that name just as `set_name` does. Saved rows and metadata are sorted by address.

Unknown metadata versions, malformed literals and invalid UTF-8 reject opening
with the path and line number. Opening never rewrites the file. Reload parses
into temporary storage and publishes only after all reads and parsing succeed;
an error retains the old rows, names, indexes and allocator. A successful reload
retains the selected lookup-index configuration. An in-memory reload still
clears the store without filesystem access.

Older readers ignore the namespaced comments. They cannot recover names stored
in this extension and can lose them on a subsequent save; use updated readers
for these files. There is no claim that the old permissive parser can reject
new metadata. Ordinary safe quoted rows retain their legacy spelling. This
change cannot reconstruct names already truncated by an earlier save/reopen.

Saving retains the existing same-directory temporary file and replacement
protocol. Inherent `save()` flushes its userspace buffer but does not fsync.
`LinksStorage::flush()` additionally synchronizes the file and, on Unix, its
parent directory. One cooperating writer must own the sidecar lock across
mutation, saving and reopening. Device/filesystem synchronization assumptions
remain; no torn-page repair or multi-writer protocol is added here.

Parsing and encoding are linear in text bytes. Persistence already sorts N
rows; M metadata names add O(M log M) sorting and temporary O(M) references.
Control-byte hex expansion is two characters per UTF-8 byte plus framing.
Atomic reload temporarily retains both old and newly loaded rows/names and
rebuilds selected indexes before publication. In-memory operations acquire no
new metadata cache or persistence work.

Run the document-label example with an absent database path:

```sh
cargo run --manifest-path rust/Cargo.toml --example text_names -- /tmp/new-labels.links
```

The example takes the advisory writer lock, flushes and reopens exact multiline,
quoted/Unicode and absent-address names. The [regression plan](case-studies/issue-138/INLINE-NAMES-PLAN.md)
and [failing-before evidence](../dev/log/issues/138/pulls/139/inline-names-before.txt)
record the native tests. Wider storage and coverage requirements remain active.
