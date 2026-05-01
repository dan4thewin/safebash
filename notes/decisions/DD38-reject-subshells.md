title: DD#38 — Reject subshells, & is the fork mechanism
status: implemented

`( cmd )` rejected. Background execution via `cmd &`. Fork is explicit.
`my_func &` forks process, child jumps to trampoline blocks.
`$!` stores last background pid. `wait` collects children.
