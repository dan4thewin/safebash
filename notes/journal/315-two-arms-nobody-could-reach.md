# 315 — Two arms nobody could reach

Continuing the branch-coverage ranking from journal 314. Two more zero-hit
arms, both genuinely dead this time, both removed.

## `$EPOCHSECONDS` in the transform

`transform/expressions.rs:172`, 0 hits against 7,055:

```rust
fn simple_expansion_to_value(var: &str, state: &VersionState) -> SsaValue {
    if var == "EPOCHSECONDS" {
        return SsaValue::CmdSubst { argv: vec![Literal("epochseconds")], … };
    }
```

It synthesised a call to the `epochseconds` intrinsic whenever the source
expanded `$EPOCHSECONDS`. But `validate.rs:1447` carries the opposite ruling
in a curated table:

```rust
("EPOCHSECONDS", "$EPOCHSECONDS does not exist in sash"),
```

Validation runs before the transform, so the rejection always wins —
`echo $EPOCHSECONDS` is SB5017 and the transform arm can never fire. The
two disagreed and the table was the deliberate one; the transform arm was
a leftover from before it existed. Removed. The `epochseconds` intrinsic
itself is untouched — this only concerned the variable spelling.

## The `attest` exemption in the SSA taint pass

`ssa_taint.rs:200`, 0 hits against 4,724:

```rust
SsaInst::Command { name, args, .. } => {
    // `attest` is the one operator allowed to receive tainted args.
    if name == "attest" {
        return;
    }
```

This one deserved more care than a count, because being wrong about it means
either a false ICE on valid code or a hole in the taint system. So it was
measured directly: an `eprintln!` in the guard, a release build, and four
spec files that between them exercise every attest form. Zero hits, including
`spec/taint.sh` which has bare `attest x [[ … ]]` at line 48.

The reason is in `transform/`. Every attest form — bare, `&&`/`||` operand
(`try_lower_attest_list`), and `if attest` — is rewritten to the if-attest
shape and lowered by `lower_if_attest` into branch machinery. None of them
survives to SSA as an `SsaInst::Command`, so the name comparison never had a
chance to be true.

Removed, with the reasoning left in place as a comment naming the condition
under which it must return: *if attest is ever lowered as a plain command, the
exemption comes back, because receiving tainted arguments is the whole point
of the operator.* A dead guard that silently stops matching is worse than no
guard, but a removed guard with no note is how the next person reintroduces
the bug.

## And one that was only untested

`lexer.rs:674`, 0 hits against 5,867 — the `self.pos += 2` in
`scan_backtick_body` that consumes an escaped backtick and sets `nested`.
Perfectly reachable; nothing in the suite had ever written one. It earns a
distinct diagnostic (`nested backticks are ambiguous — use $(...) for
nesting`) that differs from the generic SB5020 a plain backtick gets, and
that distinction was untested. Now `cov.103`.

Sweep 4667 passing, 0 failing. 35 lib units, clippy clean.

## The tally so far

Of the zero-hit arms examined across journals 311–315, the split is running
roughly even: some were missing tests, some were unreachable-by-construction,
and five were defects. What has held is that the *count alone* never decided
which — every single one needed the callers enumerated or the guard
instrumented before it could be classified. The query ranks candidates; it
does not diagnose them.

## Addendum — the third arm, purged (DG, 2026-09-14)

`transform/statements.rs`'s `is_bare_redeclare_noop` was filed rather than
removed, because it was unreachable *only* under the current SB5049 and a
narrowed SB5049 would have brought it back to life. DG settled it directly:
purge the dead function. SB5049 stays as written — one declaration per name
per scope, no exceptions — so there is no future in which a bare
redeclaration reaches the transform.

Gone with its single call site in `lower_one_declaration`, 16 lines. Sweep
unchanged at 4671 passing, 0 failing; 35 lib units; clippy clean. The task
file is deleted rather than closed — the decision is recorded here.

That closes the last of the arms surfaced by this ranking pass that were
waiting on a call. The remaining zero-hit candidates
(`is_fd_number`'s emptiness check, `block_to_emit`'s `fn_` label test,
`ssa.rs`'s `blocks.get` miss, `handled_elsewhere`'s `ENV_VARS` half) are all
defensive conditions inside functions that are themselves hot, and none of
them is coupled to an open decision.
