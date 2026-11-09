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

DD39's 2026-11-09 bounded-glob rule preserves these **literal** directory
prefixes. Only the final filename component may contain a wildcard, and
at most one `*` is allowed. Each glob opens/enumerates one directory, not a
tree. DD69 composes several expansions in source order; each expansion
is sorted bytewise and needs a literal leading `.` to select dotfiles.

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

3. **Failed `getcwd` is an operation failure.** If `getcwd` fails
   (e.g. an unlinked cwd), follow the standard DD55/DD130 error model,
   retaining the operation/cause and normal contextual diagnostic. An
   unchecked recoverable failure aborts; otherwise-legal handling may
   continue, while fatal faults remain fatal. It must **not** silently
   fall back to the bare, cwd-relative, injectable form — that would
   defeat both rationales. The original 2026-07-29 ordinary-syscall rule
   now follows DD130's recoverable/fatal distinction, not a glob-specific
   blanket abort.

4. **No matches is not a failed enumeration.** Successful enumeration
   with no matching names produces zero elements (DD69). Failed
   directory open/read operations follow the same DD130 classification
   and legal handling rules, including suppression where eligible.
   Missing-directory and not-a-directory open failures do not acquire
   an errno-only empty-success exception. A failed read must not be
   mistaken for EOF or silently publish an incomplete successful array.
   DG directed errors to follow established patterns on 2026-11-09.

DD131 provisionally supplies the command interface: `glob`/`rglob` have
an operational status and an array destination. Successful zero-match
enumeration succeeds; destination publication follows its accumulation/
scratch rules and is unchanged on an operation error. A new target
starts empty. `q-glob-semantics` owns the separate-branch trial, coordinating
with `recoverable-file-failures`, not claiming that recovery already works.

Assignments and loops remain resultless, and suppression does not reach
into helpers or operand captures. Lacking implemented handling is not
grounds to relabel an ordinary failure fatal. Both selectors retain the
absolute-path contract; `rglob` names its directory explicitly and matches
basenames rather than trying to split a path out of regex syntax.

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
  `@{a:-WORD}` (SB5032); newly produced glob elements remain **clean**.
  Accumulating them does not clear an existing destination's taint
  (DD131/DD128). Absolutization is orthogonal to both empty-disposition
  (cardinality) and taint (value).
- **DD#39** — at most one `*` in the final component; no additional
  `?`/`[]` wildcard operators, multiple stars or wildcard directory
  traversal (2026-11-09).
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

## Original implementation hook (2026-07-29)

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

## Implemented (2026-07-29; cwd-only scope)

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

This did **not** implement the literal-directory cases in the Decision.
They remain in `bug-glob-directory-portion`; the historical completion
record in `glob-absolute-paths` is qualified accordingly. The new
2026-11-09 matching/composition/error policies are also pending, not
covered by the July result.
