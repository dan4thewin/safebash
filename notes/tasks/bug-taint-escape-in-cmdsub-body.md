---
title: "TAINT ESCAPE: `check_taint` does not walk statements inside a `$( )` body"
status: open
priority: P1
kind: bug
depends: []
gates:
  - "`$(str h=$(getenv T); echo x > $h)` is SB5020, not an accepted program"
  - "`$(str h=$(getenv T); if [[ -n $h ]] { … })` is SB5020"
  - "a declaration inside a `$( )` body extends the tainted set for the rest of that body"
  - "a body-local definition does NOT leak taint out to the enclosing scope (the body is a subshell)"
  - "the walk over a `$( )` body is total over `StmtKind` — no `if let StmtKind::Command`"
---

# A tainted value reaches a redirect target with no diagnostic

Found 2026-09-28 while fixing `bug-cmdsub-declaration-misreported-as-unterminated`
— making `$(int x=1)` parse is what made these bodies reachable enough to probe.

```sh
$ cat escape.sh
/bin/echo $(str h=$(getenv TARGET); echo pwned > $h)

$ sash escape.sh -o esc          # compiles clean — no SB5020, no backstop
$ TARGET=/tmp/attacker_chose_this ./esc
$ cat /tmp/attacker_chose_this
pwned
```

An environment variable chose a filename and the program wrote to it. The
identical program at top level is correctly rejected:

```sh
str h=$(getenv TARGET)
echo pwned > $h                  # SB5020 tainted variable $h used without attest
```

## Cause

`check_cmdsub_inner_reads` (validate.rs:5737) is the *only* thing that looks
inside a `$( )` body:

```rust
for s in stmts {
    if let StmtKind::Command(c) = &s.kind {
        check_collection_reader_taint(c, tainted, diags);
        for arg in &c.args { check_expr_taint(arg, tainted, diags); }
    }
}
```

Two holes, both in those four lines:

1. **It is not total over `StmtKind`.** A `Declaration`, `If`, `While`, `For`,
   `Case` or assignment inside the body is skipped outright — including the
   `Redirected` statement that carries `> $h`.
2. **`tainted` is the *outer* set and is never extended.** `str h=$(getenv T)`
   inside the body does not add `h`, so every later read in that body is
   measured against a set that has never heard of it.

The real statement walker — `taint_stmt(stmt, tainted, cx, diags)`, used by
`taint_top_level` (validate.rs:3605) — is never applied to a body.

Same shape as the three non-total IR walks fixed in journals 430, 431 and 432.
This one is in the *primary* pass rather than a lowering pass, which is why it
is P1 rather than P2.

## Measured — how far each position gets

| program (all inside `$( … )`) | result |
|---|---|
| `str h=$(getenv T); echo x > $h` | **COMPILES — taint escapes to a filename** |
| `str h=$(getenv T); if [[ -n $h ]] { echo yes }` | **COMPILES — prints `yes`** |
| `str h=$(getenv T); echo "$h"` | SSA backstop (no SB code, no span) |
| `str h=$(getenv T); /bin/echo "$h"` | SB5020 |
| `array a=(q); str h=$(getenv T); a[0]=$h; echo "${a[0]}"` | SSA backstop |
| `str t=$(getenv T)` *outside*; `$(str z=$t; echo "$z")` | SSA backstop |

The two that compile are the two positions journal 400 already recorded as
escaping the SSA backstop — `[[ ]]` read-back and redirect targets. So the
backstop is masking most of this gap and cannot mask those two.

Note the asymmetry in the middle rows: builtin `echo` falls to the backstop
while `/bin/echo` gets a proper SB5020. Worth understanding as part of the fix
— it suggests the argument check is reached by one path and not the other.

## Fix sketch

Run the real walker over the body: `taint_stmt` for each statement, against a
**clone** of the tainted set. The clone matters both ways —

- definitions inside the body must be visible to later statements *in* that body
  (that is hole 2), and
- they must not leak outward, because a `$( )` body is a subshell: `$(str h=…)`
  does not define `h` for the enclosing program, and treating it as if it did
  would taint a name the outer scope never bound.

`check_cmdsub_inner_reads` has no `cx: &SlotCx`, so that has to be threaded to
the call sites (validate.rs:4940, 5758) or the body walk hoisted to somewhere
that already holds one.

Expect new rejections in the suite: any spec doing real work inside a `$( )`
body has never had its statements taint-checked. Each one needs reading before
it is "fixed" — a new SB5020 there is most likely correct.

## Related, but not this

- `ssa-taint-full-backstop` (P2, deferred) is about the *backstop*. This is the
  primary pass, and the two positions above prove the backstop cannot be the
  answer on its own.
- `audit-taint-bypass-report` (P2, blocked) — this belongs in that report.
