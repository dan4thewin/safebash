title: "Bug: a glob with a directory portion (`sub/*`, `./*`, `/etc/*`) silently matches nothing"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`sub/*`, `./*`, `../x/*` and `/abs/dir/*` expand to the matching entries of that directory, as DD#73's Decision and Form rule 2 specify, in both backends"
  - "each element is the lexically normalized absolute path (DD#73), e.g. `./sub1/*` gives `<cwd>/sub1/f1`"
  - "specs pin a relative, a dot-relative, a parent-relative and an absolute directory portion in both backends; each fails on the pre-fix binary"
  - "explicit glob calls and every selected sugar form share directory extraction; computed patterns receive the same validation"
  - "wildcard directory components are refused under DD39/DD131, not traversed or treated as successful empty output"

## Current ownership

Literal directory prefixes were already required by DD73. DD131's
`glob`/`rglob`/scratch package retains that obligation;
`q-glob-semantics` records implementation, evaluation and acceptance.
The old completion task explicitly limits its completed
scope to cwd globs and points here. No compiler repair accompanied that
record correction (journal 603).

## Trial implementation, 2026-11-17

Explicit `glob` and array/append/loop sugar now share directory extraction
in `glob_pattern.rs` and private enumeration in `directory.rs`, on
`glob-scratch-trial`. `globsel.1`, `.4` and `globsugar.1`, `.6`, `.7`
cover literal relative, dot/parent-relative and absolute prefixes and
composition. Static and computed wildcard-directory patterns reject;
computed operands use the same boundary. Both native and C are gated.

[Journal 647](../journal/647-coarse-directory-selectors.md) records the
matched before controls, actual syscall failures and accepted full gate.
DG accepted the package and requested landing on main on 2026-11-18
([journal 661](../journal/661-accept-glob-scratch-regex.md)), closing this
task's remaining adoption boundary. The causal description and outline below
describe the pre-trial implementation, not the current ABI.

## Historical F13 observation

Every glob whose literal part contains a `/` compiles cleanly and yields
nothing, in both backends:

| Pattern | Run in | DD#73 requires |
|---|---|---|
| `sub1/*` | `e`, holding `sub1/f1` | `…/e/sub1/f1` |
| `./sub1/*` | `e` | `…/e/sub1/f1` |
| `./*` | `e/sub1` | `…/e/sub1/f1` |
| `../sub2/*` | `e/sub1` | `…/e/sub2/f2` |
| `/tmp/…/d/a*` (array) | anywhere | `…/d/a1 …/d/a2 …/d/a3` (`len` prints 0 today) |

Bash finds the same entries (it keeps the relative spelling).

(Probes `m6`, `m7`, `m9`, `h1` from the batch-531 port's F13 glob set.)

## Why it is a bug, not a question

DD#73 settles it. Its Decision lists `sub/*`, `./*` and `/etc/*` with the
paths each must yield (`array a=(sub/*)` → `/work/sub/foo`), and its Form
rule 2 normalizes `./*`, `../x/*` and `sub/./*`. Its "Implemented
(2026-07-29)" note says only cwd globs exist and calls the directory join
"forward-looking". `glob-absolute-paths.md` was closed `done` on the same
day, but its original gate and Tests section listed `sub/*` →
`<cwd>/sub/…`, which never held. Its resolution said the truth: "Current
globs are cwd-only". The 2026-11-09 recording corrects those claims
without closing this implementation gap.

## Historical cause

Both backends pass the directory `"."` to `sash_glob` and the whole pattern
as the name pattern:

- native: `emit_glob_expansion` (`sash/src/codegen/value_handlers.rs`),
  `let dir_str = ".";`
- C: `emit_glob_c` (`sash/src/emit_c.rs`), `sash_glob(".", 1, …)`

A directory entry's name never contains `/`, so a pattern with a `/` can
never match.

## Earlier repair outline

Split the glob pattern at the last `/` before the final component. The
directory part goes to `sash_glob` as `dir` and the rest as the pattern. A leading `/`
makes it absolute; anything else joins with `getcwd`.
`sash_glob_abs_prefix` and `sash_normalize_abs` already do the joining and
normalizing for a non-`.` dir. The transform's glob recognisers
(`try_array_single_glob` in `transform/expressions.rs`, `try_dispatch_glob`
in `transform/control_flow.rs`) must carry the split pattern to both
emitters. DD39/DD131 now refuse a `*` before the last `/` (`*/f1`);
directory walking is no longer an open alternative. Validate computed
string patterns too, and preserve explicit zero-star name patterns.
`rglob` supplies its directory separately; do not reuse glob path
splitting to interpret regex syntax.

Found during the batch-531 port (F13), unrelated to the port itself. No
spec used a directory-portion glob in that census: glob.sh g.1–g.14 and
the wordsplit glob checks were all cwd patterns. Measure current
coverage/impact before implementation; this is not a fresh probe report.
