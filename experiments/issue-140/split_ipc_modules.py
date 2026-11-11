"""Moves the inline submodules of rust/src/protocol/ipc/{unix,windows}.rs into
their own files so every Rust file stays under the 1000-line CI limit.

Usage: python3 split_ipc_modules.py RUST_IPC_DIR
"""
import os
import re
import sys

root = sys.argv[1]


def dedent(lines):
    return [line[4:] if line.startswith("    ") else line.lstrip(" ") if not line.strip() else line for line in lines]


def extract(source, target_dir, modules, wrap=None):
    """modules: names of inline `mod NAME {` blocks to move to NAME.rs.
    wrap: (start_marker, end_marker, name, doc, exports) - a top-level section
    between two marker lines wrapped into a new module file."""
    with open(source) as f:
        lines = f.read().split("\n")
    os.makedirs(target_dir, exist_ok=True)
    out = []
    i = 0
    while i < len(lines):
        line = lines[i]
        m = re.match(r"^mod (\w+) \{$", line)
        if m and m.group(1) in modules:
            name = m.group(1)
            j = i + 1
            while lines[j] != "}":
                j += 1
            body = dedent(lines[i + 1 : j])
            while body and not body[-1].strip():
                body.pop()
            with open(os.path.join(target_dir, name + ".rs"), "w") as f:
                f.write("\n".join(body) + "\n")
            out.append(f"mod {name};")
            i = j + 1
            continue
        if wrap and line == wrap[0]:
            j = i + 1
            while lines[j] != wrap[1]:
                j += 1
            name, doc, exports = wrap[2], wrap[3], wrap[4]
            body = lines[i + 1 : j]
            while body and not body[0].strip():
                body.pop(0)
            while body and not body[-1].strip():
                body.pop()
            with open(os.path.join(target_dir, name + ".rs"), "w") as f:
                f.write(doc + "\n\nuse super::*;\n\n" + "\n".join(body) + "\n")
            out.append(line)
            out.append("")
            out.append(f"mod {name};")
            out.append("")
            out.append(f"pub(crate) use {name}::{{{exports}}};")
            out.append("")
            i = j
            continue
        out.append(line)
        i += 1
    with open(os.path.join(target_dir, "mod.rs"), "w") as f:
        f.write("\n".join(out))
    os.remove(source)


extract(os.path.join(root, "unix.rs"), os.path.join(root, "unix"), {"posix_mq", "sysv_mq"})
extract(
    os.path.join(root, "windows.rs"),
    os.path.join(root, "windows"),
    {"unix_socket", "copydata", "alpc"},
    wrap=(
        "// ---------------------------------------------------------------- mailslots",
        "// ---------------------------------------------------------------- WM_COPYDATA",
        "mailslot",
        "//! Mailslots: one write-only slot per direction, so the server and the\n//! client each create the slot they read and open the one they write.",
        "connect_mailslot, listen_mailslot",
    ),
)
