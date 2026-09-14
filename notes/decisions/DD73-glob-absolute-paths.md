title: DD#73 — Glob expansion yields normalized absolute paths
status: accepted

Decided with DG 2026-07-29. Amends DD#69 §1 ("one *verbatim* element per
directory entry"): a glob element is now a **normalized absolute path**.
Motivated by a demo-prep observation that nothing in the value model stops
a globbed filename from being reinterpreted as an *option* by an external
command.

## Decision

Every element produced by a glob (`*` — DD#39 star-only) is a **lexically
normalized absolute path**, not a bare directory-entry name.

- `array a=(*)` run in `/work` → `/work/foo`, `/work/bar`
- `array a=(sub/*)` → `/work/sub/foo`
- `array a=(./*)` → `/work/foo`  (no relative segment survives)
- `array a=(/etc/*)` → `/etc/passwd`  (already absolute)

## Why

Two independent rationales, correctness first (DG):

1. **Referential stability (correctness).** A bare name is a *cwd-relative
   reference*; any intervening `cd` — in the sash program, or performed by
   the external command against its own cwd — silently repoints it. An
   absolute path denotes the same file regardless of who chdirs. A bare
   name is only accidentally correct: it works until the cwd moves.

2. **Option-injection (safety).** A directory entry named `-rf` / `-i` /
   `--force` / `-o/etc/x`, splatted to an external command
   (`/bin/rm @{a:?}`), is reparsed by that command as an **option**, not a
   filename — the classic `rm *` poisoned-filename footgun. sash's
   no-word-splitting preserves argument *boundaries* but does **not**
   prevent this: the program still sees an argv entry starting with `-`. A
   leading `/` defeats all standard `getopt`/`getopt_long` parsing.

### Scope is bounded, and we say so

This closes the **glob-sourced** slice of option-injection only. A
`-`-leading value from any *other* runtime source — a blessed-tainted
`$x`, a `read -a` / `mapfile` element, a `$(…)` capture — is **not**
addressed. Globs are special only because a glob element is *always a
path*, so absolutization is a natural, lossless fix. This DD is **not**
sold as general option-injection safety; a universal "no option-ambiguous
runtime arg to an external command" policy is a separate, larger question,
deliberately left open (no universal solution is on the table yet — this
is the best partial fix available and it is a strict improvement).

## Form (DG rulings, 2026-07-29)

1. **Lexical, not `realpath`.** Absolute = `getcwd()` joined with the
   pattern's directory portion when relative; symlinks are **not**
   resolved. `realpath` would canonicalize symlinks and thereby change
   *which file* the name denotes; lexical join preserves the user's
   intended traversal.

2. **Normalized — a glob never emits a relative path.** The result carries
   no `.` / `..` / redundant `//` segments (purely textual collapse, no
   symlink resolution — composes with rule 1). `./*` → `/cwd/foo`;
   `../x/*` → the normalized parent path; `sub/./*` → `/cwd/sub/foo`.

3. **Failed `getcwd` is an ordinary syscall failure.** If `getcwd` fails
   (e.g. an unlinked cwd), abort through the standard DD#55 abort channel
   (`sash_emsg`: `<argv0>: <ctx>: <errno reason>`), exactly like a failed
   `chdir` / `open` / `write`. It must **not** silently fall back to the
   bare, cwd-relative, injectable form — that would defeat both rationales.
   (DG: "probably" handled like any failed syscall; revisit only if a
   concrete case argues otherwise.)

## Burden (deliberately flipped)

The default is safe and referentially stable (absolute). If the user wants
the short form, they **opt in**: loop the array and call `$(basename …)`
(DD#65). This inverts the prior burden — previously the author had to
remember `./ *` or a per-command `--` to be safe; now safety/stability is
the default and brevity is the explicit request.

## Interactions

- **DD#69** — amends §1 ("verbatim element" → "normalized-absolute-path
  element"). Everything else in DD#69 stands unchanged: glob *is* an array;
  inline `*` in command-argument position is a compile error (SB5029/5030);
  an external-command splat still needs the empty-disposition `@{a:?}` /
  `@{a:-WORD}` (SB5032); glob arrays remain **clean** (no taint, no bless).
  Absolutization is orthogonal to both the empty-disposition (cardinality)
  and taint (value).
- **DD#39** — star-only (`*`; no `?` / `[]` / `**`) unchanged.
- **Byte-identical-to-Bash** — deliberately relaxed here (DG owns that
  goal): using glob names as *data* (`echo`/`join`/display) now differs
  from bash (`/work/foo` vs `foo`). Accepted as the price of
  correctness-and-safety by default.
- **Relocatable artifacts** — a known trade: `/bin/tar cf a.tar @{a:?}`
  now archives absolute-pathed members (tar strips the leading `/`); use
  the `$(basename …)` opt-out when a relative/relocatable artifact is
  wanted. There is **no build-host pollution** — globs enumerate the
  *target's* cwd at *runtime* (`sash_glob` → `getdents64`), not at compile
  time, so the absolute path is the target's, computed on the target.

## Implementation hook

Single runtime site: `sash_glob` / `sash_glob_regex` in `sash-rt/src/lib.rs`
build each element (today: the bare dirent name). Prepend the
normalized-absolute directory there, reusing the existing `sash_getcwd`
(syscall 79) for the relative case; on `getcwd` failure, route to the
`sash_emsg` abort path. Codegen / emit_c / emit_janet only pass the
(now absolute) strings through — no front-end decision change (globs remain
arrays). Tracked as `notes/tasks/glob-absolute-paths.md`.

## Open / deferred (not decided here)

The general external-argument option-injection policy for non-glob runtime
sources (blessed-tainted strings, `read -a` / `mapfile` elements, `$(…)`
captures that begin with `-`). Globs are the first, easiest instance;
whether sash adopts a broader rule (auto-`--` where supported, reject
`-`-leading, a disposition, …) is unsettled.

## Tasks

- `glob-absolute-paths` — implement §Decision + §Form (runtime, specs, docs).

## Implemented (2026-07-29)

Runtime-only change in `sash-rt` (`sash_normalize_abs`, `sash_glob_abs_prefix`;
`sash_glob`/`sash_glob_regex` store `<abs-dir>/name`; `sash_getcwd` records
errno on failure; **`sash_glob_regex` was deleted by DD#108** — `sash_glob` is
now the single glob primitive and carries this behaviour alone). Current globs are cwd-only (`emit_glob_expansion` hardcodes
`dir="."`), so absolutization is a `getcwd()` prefix; the normalize/relative-
dir-join is forward-looking. `emit_c` inherits it via `libsash_rt.a`; Janet
does not expand file globs (out of scope). Specs: new `spec/glob.sh` g.1–g.6
(incl. the `-rf` option-injection pin and a `cd /` referential-stability
pin) + `wordsplit.sh` 20.6; native+c 0 regressions. Journal 135. A separate
pre-existing c-backend bug surfaced (`bug-emit-c-splat-vla-jump`), filed.
