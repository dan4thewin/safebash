title: "standalone `read` taint reports via the SSA backstop with no SB code (pipeline `read` does not)"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`read a b; attest b p?; : $a; echo $b` reports a clean SB5020 (or compiles), never 'SSA taint backstop'"
  - "the standalone and pipeline `read` shapes agree: whatever `cmd | read a b` allows, `read a b` allows"
  - "no user-reachable diagnostic is emitted without an SB code"

## Found

2026-09-08, while designing DD#98 (the `_` don't-care binder). Two `read`
shapes disagree about the same operation:

```sh
read a b                          # standalone (stdin)
attest b p?
: $a
# → tainted variable a_1 used as argument to command ':' without attest
#   (SSA taint backstop)

/usr/bin/printf "xx yy\n" | read a b   # pipeline
attest b p?
: $a
# → compiles
```

Both targets are tainted (an unattested `$b` is SB5020 in both), yet the DD#86
acknowledgment `: $a` is accepted in one shape and rejected in the other.

## Two defects, really

1. **The shapes disagree.** Nothing in DD#24 or DD#86 distinguishes them.
   Whichever is right, they should match.

2. **The diagnostic has no SB code.** The message comes from the SSA taint
   backstop (`ssa_taint.rs:509`), not the front-end taint pass, so it carries
   no `SB####` and none of the usual guidance. Compare a command substitution,
   which produces a clean `SB5020`. Every user-reachable rejection should have
   a code — spec assertions and users both key on them.

The same shape as `cmdsub-followups` gate 1 (now closed): the backstop firing
where the front-end pass should have.

## Note

DD#98 removes the *motivation* for `: $v` on a read target — write `_` instead
— but does not fix this. `: $v` remains the documented hatch for a
deliberately-unused **named** write, and it should behave the same in both read
shapes.

## Closed 2026-09-10 (backlog prune)

Re-verified 2026-09-10 — both forms now report with a proper SB code:

```
read a          -> SB5020  tainted array ${a[...]} used without attest
cat f | read a  -> SB5020  (same)
```

The asymmetry the task describes (standalone reporting via the bare SSA
backstop with no code, pipeline reporting SB5020) is gone. Closing.
