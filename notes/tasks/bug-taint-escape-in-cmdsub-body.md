---
title: "`check_taint` does not walk statements inside a `$( )` body"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "`$(str h=$(getenv T); echo x > $h)` is SB5020 with a span, not a bare backstop ICE"
  - "`$(str h=$(getenv T); if [[ -n $h ]] { … })` is SB5020 with a span"
  - "a declaration inside a `$( )` body extends the tainted set for the rest of that body"
  - "the walk over a `$( )` body is total over `StmtKind` — no `if let StmtKind::Command`"
---

> **2026-09-28 — the escape is closed; this is now a diagnostic-quality bug.**
> Both programs below are rejected at compile time (spec `bkstp.1`, `bkstp.2`).
> What closed them was the *backstop*, not this pass: `SsaInst::taint_reads`
> is now total over the enum, so `SsaInst::Test` and `SsaInst::RedirectPush`
> are checked. They are rejected with no SB code and no span, which is what
> keeps this filing open — and dropped it from P1 to P2.
>
> **The fix sketch below is wrong** where it says a `$( )` body is a subshell.
> Per the DD#86 addendum (2026-09-28) a body is an *anonymous function*: writes
> to existing globals persist, and names first introduced inside are local. So
> taint must persist OUT of a body for a name the outer scope already binds,
> and must not leak for one the body introduces. Do not clone the set
> wholesale.

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

**Superseded 2026-09-28.** Both now reject. `check_inst_uses` enumerated 4 of
19 `SsaInst` variants and ended in `_ => {}`; `Test` and `RedirectPush` were in
the wildcard. Redirects were the harder half: a `Redirect`'s target is an
unlowered `ir::Expr`, never an `SsaValue`, so no value-oriented walk could
reach it however total it was. Every row in the table above is now a rejection;
only the SB-code-and-span quality differs.

Note the asymmetry in the middle rows: builtin `echo` falls to the backstop
while `/bin/echo` gets a proper SB5020. Worth understanding as part of the fix
— it suggests the argument check is reached by one path and not the other.

## Fix sketch

Run the real walker over the body: `taint_stmt` for each statement. Scoping
follows DD#86's anonymous-function model, not a subshell's —

- definitions inside the body must be visible to later statements *in* that
  body (that is hole 2);
- a name the body introduces is body-LOCAL, so its taint must not leak outward;
- a write to a name the enclosing scope already binds DOES persist, taint and
  all, because the assignment itself persists.

`check_cmdsub_inner_reads` has no `cx: &SlotCx`, so that has to be threaded to
the call sites (validate.rs:4940, 5758) or the body walk hoisted to somewhere
that already holds one.

Expect new rejections in the suite: any spec doing real work inside a `$( )`
body has never had its statements taint-checked. Each one needs reading before
it is "fixed" — a new SB5020 there is most likely correct.

## Related, but not this

- `ssa-taint-full-backstop` (P2, deferred) is about the *backstop*, and is
  **done** as of 2026-09-28: `SsaInst::taint_reads` and `check_value_use` are
  both total. That is what closed the two escapes. It is not a substitute for
  this pass — a backstop ICE has no SB code and no span — but the claim above
  that "the backstop cannot be the answer on its own" was about coverage, and
  on coverage it now is.
- `audit-taint-bypass-report` (P2, blocked) — this belongs in that report.
