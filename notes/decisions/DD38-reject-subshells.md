title: DD#38 — Reject subshells, & is the fork mechanism
status: implemented

Sash-native `( cmd )` is a compile error. Bash-mode subshells are
mechanically coerced to sash primitives.

Bash subshells bundle three isolations into one fork. Sash decomposes
them into language and OS primitives:

| Bash subshell effect | Sash coercion |
|---|---|
| Variable isolation | Synthetic function with `local` declarations |
| Directory isolation | Save/restore cwd around body |
| Process isolation | `func &` (explicit fork) |

The compiler sees the subshell body, identifies which side effects it
contains (assignments, cd, traps), and emits the appropriate sash
primitive for each. No intent inference — the subshell is the
declaration of "these effects are temporary."

Background execution via `cmd &`. Fork is explicit and is an OS
primitive, not an isolation abstraction. `my_func &` forks process,
child jumps to trampoline blocks. `$!` stores last background pid.
`wait` collects children.
