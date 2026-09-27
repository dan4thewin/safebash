# 429 — a diagnostic that named a thing the author could not have written

*2026-09-27*

First item off the re-ranked coverage queue: `disposition_proof` in
`type_check.rs`, 11 cold lines, the head at `9b6e28c`. Three arms of a
five-arm match, and the three turned out to be three different things.

The method is plan.md's: pick a cold line, ask what input reaches it, and
**answer by execution**. Never by reading call sites. All three answers came
from a panic probe, and two of them contradicted what reading would have
suggested.

## Arm one: a live path with no spec

```rust
None => plain_read_proof(&pe.name, pe.index.as_deref(), env, quoted),
```

DD#119 already has ten specs (`14.54`-`14.63`), and `14.55` rejects an
undispositioned redirect target:

```sh
str p=d119b
/bin/echo hi > $p          # SB4030, "may be empty"
```

That looks like the case. It is not. A bare `$p` is an `ExprKind::
SimpleExpansion` and reaches `plain_read_proof` from a *different* call site
one match-arm above; only the braced `${p}` is a `ParameterExpansion` with no
operator. Probe:

```
braced ${p}    PROBE-1254-none
bare   $p      error [SB4030]
```

Two spellings of the same idea, two code paths, one spec. Spec `14.68` closes
it. Nothing was wrong — the arm does the right thing — but the two spellings
were free to drift apart and nothing would have reported it.

## Arm two: a diagnostic that fabricates syntax

```rust
Some(op) => Err(format!(
    "redirect target `${{{0}{1}…}}` may be empty — `{1}` does not \
     prove a filename …", pe.name, op)),
```

This one is a bug, and the probe named it immediately: `op=substring`.

The parser does not store the operator as the author spelled it. `${p:0:2}`
is recorded as `operator: Some("substring")` — an internal *name*
(`parse/expr.rs:1344`). Interpolating it produces:

```
redirect target `${psubstring…}` may be empty — `substring` does not
prove a filename. Use `${p:?message}` or `${p:-default}`
```

`${psubstring…}` is not sash syntax. It is not what the author wrote, it is
not what they could write, and it is not a thing the compiler accepts. The
operator vocabulary makes this general, not a one-off: `cell_string_op`,
`glob_replace`, `s_malformed`, `trust_malformed`, `arrvals_*` are all names
rather than spellings.

The house style was already there to copy. `parser.rs:3089` handles
`cell_string_op` by *describing* it — "substring / `:s/…` on an array cell
(`${name[…]:…}`)" — and never interpolates the internal token. So:

```
redirect target `${p…}` may be empty — a substring `:off:len` does not
prove a filename. Use `${p:?message}` or `${p:-default}`
```

`operator_noun` maps the known internal names and quotes anything unmapped,
so the worst future case is a puzzling noun rather than invented syntax.

## Arm three: unreachable, and the neuter proves why

```rust
Some(":~") => Err(format!("… elides to nothing …")),
```

`:~` never arrives. An elide is legal only where a token may disappear
(DD#91), a redirect target is never a variadic list position, and SB5046 in
`validate` rejects it a full pass earlier. I probed eight spellings — braced,
brace-less, quoted, cell-subject, `>`, `>>`, `<`, `2>`, `exec >` — and every
one is SB5046.

One shape is not proof of death (journal 386 is the standing reminder). Eight
that share no structure is a different claim.

DG settled what to do with proven-unreachable code in journal 375: *"Any code
that should be unreachable should halt with an ICE if it ever ran, no?"* So
the arm is now a `panic!` naming its own precondition, with the
`elide_redirect_target_is_loud_ice` unit test reaching it by direct call —
the established `*_is_loud_ice` pattern.

I want to record why an ICE is right here and not merely conventional,
because an ICE on a user-input path is normally the wrong instinct. This arm
is only reachable if SB5046 *stops rejecting elides in redirect targets*. That
is not a user writing something odd; that is a validate rule having silently
stopped applying — a compiler bug whose other consequences are unknown. A
polite diagnostic would absorb it and keep going.

The third neuter demonstrated exactly that. Flipping one `false` to `true` in
`check_elide_stmt` — disabling SB5046 for non-command statements — and
compiling `/bin/echo hi > ${p:~}`:

```
ICE: `${p:~}` reached DD#119's redirect-target proof; SB5046 should
     have rejected the elide in validate
```

The predicted cause, in the predicted words, from the predicted edit. Spec
`14.71` pins the precondition, so if SB5046 ever does stop firing there, a
spec fails before any user meets the ICE.

Incidentally, that neuter disabled SB5046 for *every* non-command statement
and exactly one spec noticed. Worth a look sometime; not today's task.

## Gate

Four specs, `14.68`-`14.71`, each paired with the control that makes it mean
something — `14.70` runs the same `${p:0:2}` outside a redirect and requires
it to *work*, so `14.69` pins the non-empty obligation rather than a ban on
the operator.

`14.69` asserts an **absence** (`$err != *'${psubstring'*`) as well as a
presence. A spec checking only `rc != 0` passed throughout the bug's life,
which is the whole reason the bug lived.

Three neuters, one failure each, each the right one: restore the fabricated
rendering → `14.69`; make the `None` arm prove true → `14.68`; disable SB5046
→ `14.71`.

Predicted 6589, swept 6589, zero failing, zero flips, 8 new ids (4 specs ×
2 modes). `disposition_proof` stays at cc 8 / cog 3 against a 3× gate of
11.5 / 5.6.
