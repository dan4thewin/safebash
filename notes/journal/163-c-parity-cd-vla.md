# 163 — c parity (batch 7): cd builtin + splat VLA-jump fix

## cd was a no-op in c

`cd` emitted `/* TODO command: cd (Cd) */` — a no-op. So `cd /tmp; …` never
changed directory, and `cd /nonexistent; echo after` printed `after` (native
aborts). A real correctness bug, latent because no c spec exercised cd
(they're in the native-only descriptive-abort block).

Added a `BuiltinCmd::Cd` arm → `emit_cd_c`: call `sash_chdir`, set
`$?`/`__test_result`, and — when unchecked — abort on failure with the DD#55
errno diagnostic `<argv0>: cd: <path>: <reason>` (`sash_emsg`), routing
through `abort_target`. No-arg `cd` succeeds silently (no $HOME magic, DD#53).
Mirrors native `handle_cd`. Verified: `cd /tmp` chdirs, `cd /bad` aborts,
`if cd /bad` is caught, `- cd /bad` is suppressed.

## Splat VLA-jump (bug-emit-c-splat-vla-jump, P2)

Making `cd` emit real code (a `goto` for the abort) surfaced a pre-existing
bug: `emit_argv_splat` declared the `@a`/`@{a:?}` argv as a **stack VLA**
(`const char* argv[n+1]`). C forbids jumping into a VLA's scope, so any
preceding statement's DD#55 abort `goto` crossing it fails to compile
(`jump into scope of identifier with variably modified type`). This regressed
glob.sh g.4 (`… cd /; /bin/cat @{a:?x}`), whose c-pass had been accidental
(cd was a no-op, so no goto).

Fixed by **heap-allocating** the splat argv via `sash_alloc` instead of a VLA
— a plain pointer has no scope-jump restriction. The bump allocator has no
free, but a splat argv is small and short-lived. Resolves the task's repro
(`touch z; array a=(x y); printf "[%s]" @{a:?x}` → `[x][y]`) and un-regresses
g.4.

## Result

Full c sweep: **0 regressions**, 0 newly-passing (cd's specs stay in the
native-only descriptive-abort block; g.4 now passes genuinely rather than by
accident). `bug-emit-c-splat-vla-jump` → done.

## Remaining descriptive-abort work

To un-guard 55.20-30 for c: (a) echo write(2)-error abort (emit_echo ignores
`sash_write` results), (b) redirect open(2)-error abort, (c) a harness change
so `_run_sash_c` captures runtime stderr into `$rerr` (currently `2>/dev/null`).
`test_bin:` is a substring of c's `./test_bin:`, so the message assertions
would match once stderr is captured.

## Files

- `sash/src/emit_c.rs` — `sash_chdir`/`sash_emsg`/`sash_alloc` externs;
  `BuiltinCmd::Cd` → `emit_cd_c`; `emit_argv_splat` VLA → heap alloc.
