title: "DD#73 — glob expansion yields normalized absolute paths"
status: done
priority: P2
kind: feature
depends: []
resolved: 2026-07-29
resolution: "Runtime-only change in sash-rt (sash_normalize_abs, sash_glob_abs_prefix; sash_glob/sash_glob_regex store <abs-dir>/name; sash_getcwd records errno on failure). Current globs are cwd-only so it's a getcwd() prefix. emit_c inherits via libsash_rt.a; janet has no file globs. spec/glob.sh g.1-6 + wordsplit 20.6; native+c 0 regressions. Journal 135. Surfaced+filed bug-emit-c-splat-vla-jump."
gates:
  - "completed cwd-only scope: a glob element is a lexically-normalized ABSOLUTE path; array a=(*) in /work yields /work/foo, not foo"
  - "literal-directory cases sub/*, ./*, ../x/* and /etc/* were not delivered by this task; bug-glob-directory-portion owns them"
  - "lexical only — symlinks are NOT resolved (no realpath); absolute = getcwd() joined with the pattern dir when relative"
  - "no relative segment (`.`/`..`/`//`) survives in any glob element (textual normalization, no symlink resolution)"
  - "original cwd-only implementation reports and aborts on failed getcwd, never falling back to bare paths; DD130/DD131 own the later recoverable handling contract"
  - "a directory entry named `-rf` splatted to an external command reaches it as `/abs/dir/-rf` (option-injection closed for globs)"
  - "DD#69 otherwise intact: inline `*`→SB5029/5030, external splat needs `@{a:?}`/`@{a:-}` (SB5032), glob arrays stay clean"
  - "original evidence: spec/glob.sh g.1-6 and wordsplit 20.6 cover cwd absolutes, option injection and chdir stability; no literal-directory completion is claimed"

# DD#73 — glob expansion → normalized absolute paths

This records DD73's **cwd-only** implementation on 2026-07-29. The
original plan below included literal-directory cases which the callers
never supplied to the runtime; they remain open in
`bug-glob-directory-portion`. Corrected on 2026-11-09 (journal 603).

DD131's provisional selectors/scratch model is a later separate-branch
trial owned by `q-glob-semantics`, not evidence that this old task
implemented those forms. The remaining sections are the historical
implementation plan, including now-retired Janet/e2e tooling.

## Hook — one runtime site

`sash_glob` and `sash_glob_regex` in `sash-rt/src/lib.rs` (~2785, ~2901)
build each element. Today they store the **bare dirent name**. Change: emit
`normalize(abs(dir))/name`, where

- `abs(dir)` = `dir` if it begins with `/`, else `sash_getcwd()` (syscall
  79, already present at ~2292) + `/` + `dir`;
- `normalize` = textual collapse of `.` / `..` / `//` segments only (no
  symlink resolution — lexical, per DD#73 rule 1);
- on `sash_getcwd` failure, route to the `sash_emsg` abort path (DD#55),
  same as `chdir`/`open`/`write` failures — do **not** fall back to bare.

Note the current `dir` argument is the pattern's directory portion (`.` /
empty for a bare `*`); confirm how the caller
(`codegen/value_handlers.rs`, `arr=(*.sh)` path) passes `dir` and whether
any dir-prefixing happens above the runtime, so the absolute join is
applied exactly once.

`#![no_std]` constraints: build the joined/normalized path into a fixed
buffer (mind `PATH_MAX`/4096 like the existing `dir_path` buffer); an
overlong result is itself a syscall-style failure (name too long) — abort,
don't truncate.

## Backends

`emit_c` (`sash_glob` mirror) and `emit_janet` must produce the same
absolute elements so the differential oracles stay aligned. Prefer sharing
the runtime `sash_glob` semantics; if janet globs independently, apply the
same abs+normalize+getcwd-fail rule there.

## Tests

**Completion boundary:** the actual delivered evidence is g.1-g.6 and
wordsplit 20.6, recorded in the resolution above. Directory-prefix cases
in this original test plan were not delivered; use
`bug-glob-directory-portion` for their current gates.

- **spec** (`spec/glob*.sh` — there is a glob/glob_splat suite): pin
  `array a=(*)` → absolute; `sub/*` → `<cwd>/sub/…`; `./*` and `../*`
  normalized; `/etc/*` unchanged; a `-rf`-named entry splatted to an
  external is `/abs/-rf`. Run native+c+janet.
- **e2e** (`sash/tests/e2e_tests.rs`): a run-and-check that globs a temp
  dir (incl. a `-rf` file) and asserts the elements are absolute and the
  external sees a `/`-leading arg.
- getcwd-failure path: hard to trigger portably; at minimum a unit check of
  the normalize helper + a note. 0 native spec regressions is the gate.

## Docs

- `notes/decisions/DD69-…md` §1 — cross-ref DD#73 (done in the same change
  or already stubbed).
- `docs/SUBSET.md` Globs section (~213) — state that `*` yields normalized
  absolute paths, with the `$(basename …)` opt-out.

## Watch

- Byte-identical-to-bash is intentionally relaxed here (DD#73) — glob
  data-uses now differ from bash; update/relax any spec that asserted bare
  glob names as bash-oracle output.
- Relocatable-artifact trade (tar etc.) is accepted; the opt-out is
  `$(basename …)` in a loop.
