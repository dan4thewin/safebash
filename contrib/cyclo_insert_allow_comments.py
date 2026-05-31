#!/usr/bin/env python3
"""Insert `// cyclo-allow: <rationale>` comments above each whitelisted fn.

Reads .cyclo-allow at repo root, looks up each fn in the indexnudge
database, and prepends a one-line comment above its definition. Skips
entries that already have the comment.

Overrides exist for ambiguous symbols (same fn name in same file but
different impl blocks).
"""

from __future__ import annotations
import os
import re
import sqlite3
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parents[1]
ALLOW = REPO / ".cyclo-allow"
DB = REPO / ".indexnudge-rust.db"

# (file, name) -> explicit start_line if multiple impls share a name.
OVERRIDES = {
    ("sash/src/ssa.rs", "display"): 222,  # ArithOp::display (not VarRef::display)
}


def load_allow() -> list[tuple[str, str, str]]:
    out = []
    for raw in ALLOW.read_text().splitlines():
        line = raw.split("#", 1)
        spec = line[0].strip()
        rationale = line[1].strip() if len(line) > 1 else ""
        if not spec or "::" not in spec:
            continue
        path, fn = spec.split("::", 1)
        out.append((path.strip(), fn.strip(), rationale))
    return out


def lookup_line(conn, file: str, name: str) -> int | None:
    if (file, name) in OVERRIDES:
        return OVERRIDES[(file, name)]
    rows = conn.execute(
        "SELECT start_line FROM symbols WHERE file=? AND name=?",
        (file, name),
    ).fetchall()
    if not rows:
        print(f"warn: {file}::{name} not found in index", file=sys.stderr)
        return None
    if len(rows) > 1:
        print(
            f"warn: {file}::{name} has {len(rows)} matches — add to OVERRIDES",
            file=sys.stderr,
        )
        return None
    return rows[0][0]


def insert_comments() -> None:
    conn = sqlite3.connect(str(DB))
    entries = load_allow()

    # Group by file, sort by line DESCENDING so insertions don't shift later targets.
    by_file: dict[str, list[tuple[int, str, str]]] = {}
    for file, fn, why in entries:
        line = lookup_line(conn, file, fn)
        if line is None:
            continue
        by_file.setdefault(file, []).append((line, fn, why))

    for file, hits in by_file.items():
        path = REPO / file
        text = path.read_text().splitlines()
        hits.sort(reverse=True)  # high line numbers first
        changed = False
        for line, fn, why in hits:
            # line is 1-indexed; convert to 0-indexed
            idx = line - 1
            if idx <= 0 or idx >= len(text):
                continue
            # Idempotency: look upward past blank lines and doc comments. If a
            # cyclo-allow comment is already present above this fn, skip.
            j = idx - 1
            already = False
            while j >= 0:
                s = text[j].strip()
                if s.startswith("// cyclo-allow:"):
                    already = True
                    break
                if s == "" or s.startswith("///") or s.startswith("//!"):
                    j -= 1
                    continue
                break
            if already:
                continue
            # Sanity check: the target line should actually look like a fn def.
            # If the index is stale and points at a cyclo-allow comment we just
            # inserted on a previous run, skip silently.
            if text[idx].strip().startswith("// cyclo-allow:"):
                continue
            if "fn " not in text[idx]:
                print(
                    f"warn: {file}:{line} does not look like a fn def: {text[idx].strip()!r}",
                    file=sys.stderr,
                )
                continue
            # Preserve indentation of the fn line.
            fn_line = text[idx]
            m = re.match(r"^(\s*)", fn_line)
            indent = m.group(1) if m else ""
            comment = f"{indent}// cyclo-allow: {why}"
            text.insert(idx, comment)
            changed = True
            print(f"  inserted at {file}:{line} ({fn})")
        if changed:
            path.write_text("\n".join(text) + "\n")
            print(f"wrote {file}")


if __name__ == "__main__":
    insert_comments()
