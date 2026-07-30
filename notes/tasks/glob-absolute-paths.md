---
title: "DD#73 — glob expansion yields normalized absolute paths"
status: done
priority: P2
kind: feature
depends: []
resolved: 2026-07-29
resolution: "Runtime-only change in sash-rt (sash_normalize_abs, sash_glob_abs_prefix; sash_glob/sash_glob_regex store <abs-dir>/name; sash_getcwd records errno on failure). Current globs are cwd-only so it's a getcwd() prefix. emit_c inherits via libsash_rt.a; janet has no file globs. spec/glob.sh g.1-6 + wordsplit 20.6; native+c 0 regressions. Journal 135. Surfaced+filed bug-emit-c-splat-vla-jump."
gates:
  - "a glob element is a lexically-normalized ABSOLUTE path: `array a=(*)` in /work yields /work/foo (not foo); `sub/*` → /work/sub/foo; `./*` → /work/foo; `../x/*` → normalized parent; `/etc/*` unchanged"
  - "lexical only — symlinks are NOT resolved (no realpath); absolute = getcwd() joined with the pattern dir when relative"
  - "no relative segment (`.`/`..`/`//`) survives in any glob element (textual normalization, no symlink resolution)"
  - "failed getcwd aborts via the DD#55 channel (`sash_emsg`: `<argv0>: <ctx>: <errno>`) — NEVER a silent fallback to the bare/relative form"
  - "a directory entry named `-rf` splatted to an external command reaches it as `/abs/dir/-rf` (option-injection closed for globs)"
  - "DD#69 otherwise intact: inline `*`→SB5029/5030, external splat needs `@{a:?}`/`@{a:-}` (SB5032), glob arrays stay clean"
  - "docs updated (DD#69 §1 cross-ref, SUBSET.md Globs section); specs (spec/glob*.sh) + e2e cover the forms above; 0 native spec regressions"
---

# DD#73 — glob expansion → normalized absolute paths

Implements DD#73 (`notes/decisions/DD73-glob-absolute-paths.md`). All four
design sub-decisions are ratified (lexical / normalize / getcwd-failure =
syscall-failure / burden flipped to `$(basename)` opt-out); this task is
mechanics + tests + docs.

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
