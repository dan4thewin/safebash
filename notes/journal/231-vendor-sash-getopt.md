# 231 — Vendor `sash-getopt` (NUL-separated, unquoted)

Second half of DG's directive (2026-09-02): after dropping `readc` (journal
230), "pull in the absolute minimum from ~/contrib/util-linux to vendor getopt,
patch to default to NULL separation and no quoting and build as sash-getopt
binary."

## Why

The sanctioned sash idiom for consuming `getopt`'s option canonicalization is
**NUL-splitting**, not parsing shell-quoting. A NUL byte can't occur inside an
argv element, so a NUL-separated option stream splits unambiguously with *no*
quoting to parse and *no* `eval`. Stock util-linux getopt only emits
space-separated, shell-quoted strings (`-s sh|csh`), so we vendor a patched
copy that emits NUL-separated, unquoted fields.

## What was vendored

`sash-getopt/sash-getopt.c` — based on `util-linux/misc-utils/getopt.c`
(Frodo Looijaard, GPL-2+; the license header is preserved). The **absolute
minimum**: the getopt(3)/getopt_long(3) option-parsing core plus the
`-o`/`-l`/`-n`/`-a`/`-q`/`-Q`/`-T`/`-U` flags. The util-linux internal
dependencies were stripped and replaced with small inline helpers:

- `nls.h` (`_()` i18n, `setlocale`/`textdomain`) → removed.
- `xalloc.h` (`xmalloc`/`xreallocarray`/`xstrdup`) → local malloc/realloc/strdup
  wrappers with an OOM check.
- `strutils.h` (`ul_strconcat`) → a local `str2cat`.
- `closestream.h` (`close_stdout_atexit`) → removed.
- `err.h`/`errtryhelp`/`print_version`/`usage` → a local `parse_error` + a short
  `usage`.

## The patch (output format)

The output changed from "space-separated, shell-quoted string + trailing
newline" to **NUL-separated, unquoted fields, no trailing newline**:

- a single `emit(s)` writes `s` then a `\0`; `emit_short`/`emit_long` build the
  `-x` / `--long` tokens;
- `print_normalized` (the shell-quoting function) is gone — every token
  (`-x`, `--long`, an option value, `--`, a positional) is emitted raw via
  `emit`;
- the `-s shell` and `-u` modes (shell-specific quoting) are gone — accepted
  and ignored for drop-in compatibility.

Example — `sash-getopt -o ab: -- -a -b val 'has space'`:

```
-a\0-b\0val\0--\0has space\0
```

`has space` is ONE field; stock getopt would emit ` -a -b 'val' -- 'has space'`.

## Build + test

- `mkfile`: a `target/release/sash-getopt` rule (`cc -O2`), added to `build`;
  a `sash-getopt-test` target added to `test` (mirrors `sash-regex`).
- `contrib/test-getopt.sh` — 5 smoke checks (short opts, a space-bearing arg
  stays one field, long options, an option value, trailing positionals),
  rendering each NUL as `|`. All pass.

The binary is built into `target/` (gitignored); only the source, build rule,
and test are committed.

## Consumer — the open piece

The other half of the idiom is a **NUL-delimited array read** to split
`sash-getopt`'s output into an array. The runtime is already ready:
`sash_split_into_array(ptr, len, delim, table)` is length-based (NUL-safe) and
splits on any delim byte, and `read -a arr` routes to it with
`delim = flags.delim`. **What's missing is the surface**: how a user requests a
NUL delimiter. `read -z` is currently *rejected* (SB5015, spec cov.20), and the
NUL-read surface (`-z`? `-d ''`? a distinct form? the `@arr` splat?) is the
open question in `switch-free-read-family`. Left for a DG decision — flagged
there and reported.
