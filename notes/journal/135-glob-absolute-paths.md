# 135 — DD#73: glob expansion yields normalized absolute paths

Implemented DD#73 (decided this session — `notes/decisions/DD73-…md`).
A glob element is now `<getcwd>/name`, not the bare dirent name.

## Why (recap)

- **Correctness:** a bare name is a cwd-relative reference; any intervening
  `cd` (in the program, or by the external command against its own cwd)
  silently repoints it. An absolute path is stable.
- **Safety:** a directory entry named `-rf` / `--force`, splatted to an
  external command, is otherwise reparsed as an *option*. `/abs/-rf` starts
  with `/` and defeats getopt. Closes the glob-sourced slice of
  option-injection (explicitly not sold as a general fix — DD#73 §scope).

DG waived byte-identical-to-bash here and flipped the burden: absolute is
the safe/stable default, `$(basename …)` in a loop is the opt-out.

## Implementation — one runtime site

sash globs are **runtime** (`sash_glob` → `getdents64` on the *target*),
so there is no build-host pollution; a `getcwd()` for absolutization is the
target's runtime cwd. Current globs are also **cwd-only** (`emit_glob_expansion`
hardcodes `dir="."`), so absolutization is effectively a `getcwd()` prefix.

`sash-rt/src/lib.rs`:
- `sash_normalize_abs(input, out)` — lexical `.`/`..`/`//` collapse, NO
  symlink resolution (DD#73 rule: lexical, not realpath).
- `sash_glob_abs_prefix(dir, out)` — normalized absolute directory prefix;
  relative `dir` resolved via `sash_getcwd`; a failed `getcwd` aborts
  through the DD#55 channel (`sash_emsg` → `sash_exit(1)`), never a silent
  fallback to the bare/injectable form.
- `sash_glob` / `sash_glob_regex` — compute the prefix once, store
  `<prefix>/name` per entry instead of the bare name.
- `sash_getcwd` — now records `SASH_LAST_ERRNO` on failure so the abort
  renders a real reason.

No compiler/front-end change (globs remain arrays); `emit_c` inherits the
fix by linking `libsash_rt.a` (it declares `extern sash_glob`). Janet does
not expand file globs, so it is out of scope (native+c only).

## Tests (spec-first)

New `spec/glob.sh` (g.1–g.6, native+c): element is `<cwd>/name` (g.1);
option-injection closed — a `-rf.zz` file → `<cwd>/-rf.zz`, starts with `/`
(g.2); array-literal glob absolute via for-in (g.3); **referential
stability** — glob, `cd /`, then `cat @{a:?}` still reads the file (g.4);
zero-match still empty (g.5); multi-match all absolute (g.6). Updated
`wordsplit.sh` 20.6 (bare → `<cwd>/only.www`). native+c 0 regressions;
6 new passes ×2 modes.

## Found while testing (filed, not fixed)

`bug-emit-c-splat-vla-jump` — a c-backend bug independent of DD#73: an
external `@{a:?}` / `@{a:-}` splat *after a preceding statement* emits C
with a `goto` that jumps into a VLA scope (`jump into scope of identifier
with variably modified type`), so gcc rejects it. Reproduces with a literal
`array a=(x y)` (no glob); native is correct. The DD#73 g.3 spec was
written to observe via for-in so it does not depend on this bug.
