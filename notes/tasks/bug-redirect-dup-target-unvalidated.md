---
title: "BUG — redirect fd-dup target isn't validated; malformed/non-numeric targets silently dup to fd 2"
status: pending
priority: P2
kind: bug
depends: []
gates:
  - "`echo hi >&foo` and `echo hi 2>&1x` reject at compile time (not silently dup to fd 2)"
  - "`>&-` / `N>&-` either closes the fd (if implemented) or rejects — never dups to fd 2"
---

## Symptom (native; same silent-degrade class as the 2026-08-29 param work)

`>&`/`<&` (fd-dup) targets are parsed as a general word and never validated as a
numeric fd. `emit_dup_output` (sash/src/codegen/redirects.rs:226-228) then does:

```rust
let target_fd: i64 = match &target.kind {
    ExprKind::Literal(s) => s.parse().unwrap_or(2),   // non-numeric → 2
    _ => 2,                                            // non-literal → 2
};
```

So every malformed / non-numeric dup target **silently dups to fd 2**:

| input | sash (native) | bash |
|-------|---------------|------|
| `echo hi >&foo` | stdout→fd2 (stderr) | redirect stdout+stderr to file `foo` |
| `echo hi >&999x` | stdout→fd2 | ambiguous redirect (error) |
| `echo hi 2>&1x` | trailing `x` dropped, dup2(2,2) no-op | ambiguous redirect (error) |
| `echo hi >&-` | stdout→fd2 | **close** fd 1 |

A second silent fallback sits at redirects.rs:401 (`suffix.parse().unwrap_or(0)`)
for the leading-fd parse.

The compiler is doing something (dup to stderr) for input it doesn't understand
— exactly the ambiguity DG flagged ("if the compiler doesn't know what something
does, that's very bad"). The sibling operator-dispatch parsers were probed at the
same time and are clean: arith → SB4035/SB4020, `[[ ]]` → SB4014, subscript →
SB4041. Only redirects have the silent fd-2 fallback.

## Fix (needs a DG scope decision)

Remove the `unwrap_or(2)` / `_ => 2` fallback. Then, for a non-numeric dup
target, the fork is:
- **Reject-only** — `>&N` / `N>&M` / `<&N` are the only fd-dup forms; anything
  else (non-numeric, trailing garbage, `-`, a filename) → a clean SBxxxx reject.
  Simplest, matches the param-expansion no-ambiguity fix (SB4034/SB4047).
- **+ close** — additionally implement `>&-` / `N>&-` (close the fd), a common
  bash form; reject the rest.
- **+ file** — additionally implement `>&file` / `&>file` (redirect stdout+stderr
  to a file); reject trailing-garbage / bad-fd.

Whichever, the invariant is: a fd-dup target is a valid fd number (and the
implemented special forms) or it rejects — never a silent fd-2 fallback.

## Found

2026-08-29, probing the sibling operator-dispatch parsers after the
param-operator silent-degrade fixes (journals 209-211), in answer to DG's
"were there more bugs before resuming non-executed function checks?"
