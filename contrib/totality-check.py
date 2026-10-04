#!/usr/bin/env python3
"""Enforce that self-declared total match walkers stay wildcard-free.

Every taint escape this project has found sat behind a wildcard match arm;
none has been found behind an exhaustive one. The A/B was inside a single
file: `classify_inst_def` (exhaustive over all 19 `SsaInst` variants, no
escapes) against `check_inst_uses` (`_ => {}`, 4 of 19, both live escapes).

So the convention is that a walk which must see every variant says so in its
doc comment, and rustc's exhaustiveness check does the enforcing. A wildcard
arm silently disarms that. This gate makes adding one a commit failure.

Opt in by writing the marker below in the function's doc comment:

    /// **Total over `ExprKind` on purpose — do not add a wildcard.**

Usage: totality-check.py [FILE...]   (default: sash*/src/**/*.rs)
Exit 0 clean, 1 on a violation.
"""

import re
import sys
from pathlib import Path

MARKER = "do not add a wildcard"

# A bare wildcard arm: `_ =>`, `| _ =>`, `_ if cond =>`, `other =>`, `rest =>`.
# `Some(_) =>` and `(_, x) =>` are NOT wildcards — the `_` is a field there,
# and the enum is still matched exhaustively.
WILDCARD = re.compile(r"^\s*(?:\|\s*)?(_|other|rest|_other)\b[^=>]*=>")

FN_START = re.compile(r"^\s*(pub(?:\([^)]*\))?\s+)?(async\s+)?(unsafe\s+)?fn\s")


def strip_noise(line: str) -> str:
    """Remove line comments and string literals so brace counting is sane."""
    line = re.sub(r'"(?:\\.|[^"\\])*"', '""', line)
    line = re.sub(r"'(?:\\.|[^'\\])'", "''", line)
    return line.split("//", 1)[0]


def function_body(lines: list[str], fn_line: int) -> tuple[int, int]:
    """Line range [start, end] of the function whose signature begins at
    `fn_line`, located by brace depth."""
    depth = 0
    seen = False
    for i in range(fn_line, len(lines)):
        for ch in strip_noise(lines[i]):
            if ch == "{":
                depth += 1
                seen = True
            elif ch == "}":
                depth -= 1
                if seen and depth == 0:
                    return fn_line, i
    return fn_line, len(lines) - 1


def marker_sites(lines: list[str]) -> list[tuple[int, int]]:
    """`(marker_line, end_of_doc_run)` for every doc comment carrying the
    marker.

    The marker is matched against the *joined* doc-comment run, not against
    single lines. rustfmt wraps a doc comment at the column limit wherever the
    text happens to fall, so `command_args_elidable` opted in across a line
    break — `do not add a` / `/// wildcard.` — and a per-line search did not
    see it. The function was ungated from the day it was written, and the
    phrasing that hid it is the phrasing this file's own docstring recommends.
    """
    sites = []
    i = 0
    while i < len(lines):
        if not lines[i].strip().startswith("///"):
            i += 1
            continue
        j = i
        while j < len(lines) and (
            lines[j].strip().startswith("///") or lines[j].strip().startswith("#[")
        ):
            j += 1
        doc = " ".join(l.strip().lstrip("/").strip() for l in lines[i:j])
        if MARKER in re.sub(r"\s+", " ", doc):
            sites.append((i, j))
        i = j
    return sites


def check(path: Path) -> list[str]:
    lines = path.read_text().splitlines()
    problems = []
    for n, end_doc in marker_sites(lines):
        fn = next(
            (i for i in range(end_doc, len(lines)) if FN_START.match(lines[i])), None
        )
        if fn is None:
            problems.append(f"{path}:{n + 1}: marker with no following `fn`")
            continue
        start, end = function_body(lines, fn)
        name = lines[start].strip()
        for i in range(start, end + 1):
            if WILDCARD.match(strip_noise(lines[i])):
                problems.append(
                    f"{path}:{i + 1}: wildcard arm `{lines[i].strip()}` in a "
                    f"function that declares itself total\n"
                    f"    ({name}  — marker at {path}:{n + 1})"
                )
    return problems


SELF_TEST = {
    "wrapped-marker": (
        "/// **Total over `Thing` on purpose — do not add a\n"
        "/// wildcard.**\n"
        "fn classify(t: &Thing) -> u8 {\n"
        "    match t {\n"
        "        Thing::A => 1,\n"
        "        _ => 0,\n"
        "    }\n"
        "}\n"
    ),
    "one-line-marker": (
        "/// **Total on purpose — do not add a wildcard.**\n"
        "fn classify2(t: &Thing) -> u8 {\n"
        "    match t {\n"
        "        Thing::A => 1,\n"
        "        other => 0,\n"
        "    }\n"
        "}\n"
    ),
}


def self_test() -> list[str]:
    """Prove the gate can fail before trusting it to pass.

    A gate that cannot fail is not a gate, and this one could not for the
    wrapped-marker case from the day it was written — rustfmt put a line break
    inside the marker in `command_args_elidable` and a per-line search went
    blind. Both spellings must be caught, so the cheapest guard against a
    repeat is to run them on every invocation.
    """
    import tempfile

    failures = []
    with tempfile.TemporaryDirectory() as d:
        for name, text in SELF_TEST.items():
            p = Path(d) / f"{name}.rs"
            p.write_text(text)
            if not check(p):
                failures.append(
                    f"self-test `{name}`: the gate did not flag a wildcard in a "
                    f"function that declares itself total"
                )
    return failures


def main() -> int:
    broken = self_test()
    if broken:
        print("totality gate: THE GATE ITSELF IS BROKEN\n")
        for b in broken:
            print(f"  {b}")
        print(
            "\n  It would have reported every file as clean. Fix the gate\n"
            "  before trusting this run."
        )
        return 2
    args = [Path(a) for a in sys.argv[1:]]
    if not args:
        args = sorted(
            p
            for d in Path(".").glob("sash*/src")
            for p in d.rglob("*.rs")
        )
    problems = [p for f in args if f.suffix == ".rs" and f.exists() for p in check(f)]
    if problems:
        print("totality gate: a self-declared total walker grew a wildcard\n")
        for p in problems:
            print(f"  {p}")
        print(
            "\n  A walk that must see every variant relies on rustc's\n"
            "  exhaustiveness check as its enforcement. A wildcard disarms it.\n"
            "  Either handle the variant or drop the marker from the doc comment."
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
