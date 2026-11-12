title: DD#38 — Reject subshells, & is the fork mechanism
status: implemented

1. Forking is explicit through background execution and process pipelines;
   lexical scope is governed separately by DD86.
2. A background function runs in its own child; `$!`/`getcpid` identifies that
   child under DD126's latch rules. Single external jobs exec directly (DD55).
3. Explicit `wait` reaps one child and returns its pid; draining children
   requires a loop (DD126).

---

Sash-native `( cmd )` is a compile error. Bash-mode subshells are
mechanically coerced to sash primitives.

Bash subshells bundle three isolations into one fork. Sash decomposes
them into language and OS primitives:

| Bash subshell effect | Sash coercion |
|---|---|
| Variable isolation | ~~Synthetic function with `local` declarations~~ (DD86 retires this scope model) |
| Directory isolation | Save/restore cwd around body |
| Process isolation | `func &` (explicit fork) |

~~The compiler sees the subshell body, identifies which side effects it
contains (assignments, cd, traps), and emits the appropriate sash
primitive for each. No intent inference — the subshell is the
declaration of "these effects are temporary."~~ DD57 places Bash conversion
in the separate converter, not the native compiler.

Background execution via `cmd &`. Fork is explicit and is an OS
primitive, not an isolation abstraction. `my_func &` forks process,
child jumps to trampoline blocks. `$!` stores last background pid.
`wait` collects children — one per call, returning the reaped pid (DD#126);
"wait for all" is a loop, not a flag.
