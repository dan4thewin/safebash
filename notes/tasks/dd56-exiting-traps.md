title: DD#56 — implement exiting traps (trap exit SIG)
status: ready
depends:
  - dd56-flag-traps
  - bug-exit-trap-wrong-scope
severity: feature
gates: [56.2, 56.4, 56.5, 56.10]
  - tests
  - commit

`trap exit SIG` registers a flag trap AND tells the compiler to
insert signal checks at defined points. When check fires, jump to
EXIT block.

### Compiler state

Track which signals are in the "exiting set" at each point in the
program. The set changes when `trap exit SIG` or `trap : SIG` or
`trap - SIG` is encountered.

### Check insertion

When the exiting set is non-empty, insert checks at:
- Before external commands
- Before function calls
- Before function returns
- At loop tops (while/for/until)

Check: load `sash_signum`, if nonzero and in exiting set, goto
EXIT block.

### EXIT block rework

Supersedes current implementation (inlined before every Return).
Single block at end of `_start`:
- Reached from: program exit path, `exit` builtin, exiting trap checks
- Receives signal number via `sash_signum` (or 0 for normal exit)
- Programmer writes dispatch logic (case/esac on $(signum))
- Every path must end with `exit`

### Changing traps mid-program

`trap exit SIGINT` then later `trap : SIGINT` — compiler stops
inserting checks for SIGINT from that point. The exiting set is
a compile-time concept tracked through the program's control flow.

### Interaction with DD#55

EXIT block follows normal error model. `- cmd` available for
cleanup commands that might fail.
