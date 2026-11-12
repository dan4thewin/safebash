title: DD#56 — implement signal SIG EXIT
status: done
depends: [dd56-flag-traps, bug-exit-trap-wrong-scope]
severity: feature
gates: [56.8, 56.9]

`signal SIG EXIT` registers a flag trap AND inserts compiler check
points. When check fires, jump to END block.

### Check insertion
When exiting set is non-empty, insert checks at:
- Before external commands
- Before function calls
- Before function returns
- At loop tops

Check: load SASH_SIGNUM, if nonzero and in exiting set, goto end_block.

### Changing disposition mid-program
`signal INT EXIT` then later `signal INT FLAG` — compiler stops
inserting checks from that point.
