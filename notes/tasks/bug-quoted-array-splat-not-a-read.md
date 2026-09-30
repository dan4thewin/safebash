---
title: "`for v in \"@g\"` reports SB5050 dead-variable — the quoted splat is not counted as a read"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "`array g=(a b); for v in \"@g\" { echo $v }` compiles, or the quoted splat is rejected with a message that says why"
  - "the unquoted `@g` form is unaffected"
---

# SB5050's read-tracking does not see through `DoubleQuoted`

Found 2026-09-30 while fixing `bug-taint-off-ignored-on-for-in-splat`, as a
control probe. **Unrelated to taint** — it reproduces on a clean array, and
reproduces identically on the pre-fix binary, so it is not a regression.

```sh
array g=(a b)
for v in "@g" { /bin/echo "[$v]" }
# SB5050: variable `g` is assigned but never read (dead variable)
```

Add any *other* read of `g` and the same loop compiles and runs correctly,
printing `[a]` `[b]`:

```sh
array g=(a b)
/bin/echo $(len g)
for v in "@g" { /bin/echo "[$v]" }   # fine
```

So the splat itself works end to end. The **only** defect is that the
dead-variable pass does not count a quoted splat in a `for`-in source as a
read of the array.

## Why it is a genuine mismatch, not an unsupported spelling

`detect_array_iter` (`sash/src/transform/control_flow.rs`) deliberately
recurses through a single-part `DoubleQuoted`:

```rust
ExprKind::DoubleQuoted(parts) if parts.len() == 1 => detect_array_iter(&parts[0].kind),
```

The transform therefore *intends* the quoted form to be an array iteration —
and lowers it as one. Two passes disagree about whether the same word is a
read. `"@{g:!}"` fails the same way for the same reason.

## The open question

Whether `"@g"` should be a supported spelling at all is a language decision,
not a scanner one — sash spells the splat `@g`, and bash's quoted form is
`"${a[@]}"`, which is a different surface. Three options:

1. Teach the dead-variable pass to see through `DoubleQuoted`, matching what
   the transform already does. Smallest fix; keeps the two passes in
   agreement.
2. Reject `"@g"` explicitly with a message naming the real problem ("a quoted
   array splat is not a splat; write `@g`"), and drop the `DoubleQuoted` arm
   from `detect_array_iter` so no pass claims to support it.
3. Leave it.

Option 2 is the larger change but removes a spelling that currently works
only by accident of another read existing. DG decides.

## Where

- `sash/src/validate.rs` — the SB5050 dead-variable pass (read collection).
- `sash/src/transform/control_flow.rs` — `detect_array_iter`'s `DoubleQuoted` arm.
