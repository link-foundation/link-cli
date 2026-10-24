# Native legacy version sidecar markers

`VersionControlDecorator` stores named metadata links alongside its transaction
journal. The Rust and C# marker encoders share
[wire fixtures](version-metadata-vectors.json); their native database file
formats are separate. This format concerns the optional legacy decorator.
The staged engine uses its checksummed revision journal instead.

New branch markers have five fields after the existing prefix:

```text
__vc:branch:2|<name>|<parent-or-dash>|<fork-sequence>|<head-sequence>
```

Names contain four uppercase hexadecimal digits for each UTF-16 code unit,
in order, without a byte-order mark. For example, `A😀` is
`0041D83DDE00`. The empty field encodes an empty string; `-` encodes a null
parent. Hexadecimal readers also accept lowercase digits. Sequences use signed
64-bit decimal integers without whitespace; public branch operations validate
nonnegative sequences. The five-field shape and encoded names prevent a name
from injecting the parent, fork or head separators. C# preserves every code unit
of its string type; Rust rejects unpaired surrogates because they cannot form a
Rust string. Both preserve valid Unicode, embedded NUL and newline exactly.

Readers retain the original branch format:

```text
__vc:branch:<name>:parent=<parent-or-empty>:fork=<sequence>:head=<sequence>
```

A legacy name can itself begin with `2|`; a nonnumeric suffix prevents it from
being mistaken for a valid new marker. Existing unambiguous legacy records are
read without rewriting their metadata links. Old records that already placed
separator text in both a branch and parent can be intrinsically ambiguous;
this change does not claim to reconstruct their original names. New records
avoid that ambiguity. Older binaries do not understand version-two branch
markers, so retain a backup before upgrading a store that must be downgraded.

Tag writers retain their original format:

```text
__vc:tag:<name>=<sequence>
```

Readers split at the final equals sign. The numeric sequence cannot contain an
equals sign, so the full name remains unambiguous even when it contains `=`.
This also correctly reads such tags created by an older writer.

Current-branch and transition-attribution markers retain their existing format:

```text
__vc:current=<name>
__vc:trans:<sequence>:branch=<name>
__vc:applied=<sequence>
```

Those name fields occupy the entire remaining suffix. There is no later field
for a name delimiter to corrupt. This format change preserves existing marker
prefixes, admission behavior, fault-injection phases and public API signatures.
Encoding costs apply only when the optional version decorator writes metadata;
branch name fields occupy four characters per UTF-16 code unit.
