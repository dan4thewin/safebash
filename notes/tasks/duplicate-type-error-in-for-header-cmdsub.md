status: next
priority: P3
depends:
gates:

# `for k in $(keys …)` reports its type error twice

## The symptom

```sh
array a=(x)
for k in $(keys a) { echo "$k" }
```

```
q.sash:2:12: error [SB4030] `keys` needs a map — `a` is an indexed array — …
q.sash:2:12: error [SB4030] `keys` needs a map — `a` is an indexed array — …
```

Identical message, identical span, twice. Both are correct; there is just one
too many.

It is specific to a `$( )` inside a **for-in header**. The same operand error
reached through an assignment reports once:

| shape | count |
|---|---|
| `for k in $(keys a) { … }` | **2** |
| `keys a` as a statement | 1 (SB5073) |
| `str j=$(join s)` | 1 |
| `echo "$(len s)"` | 1 |

## Why

Two walkers both reach a for-header's `$( )` body.

1. `check_for` (`type_check.rs:2138`) walks `f.words` through
   `check_subscript_types`, whose `ExprKind::CommandSubstitution` arm calls
   `check_subscript_in_stmt` — which calls `check_keys_operand`.
2. `check_stmt` (`type_check.rs:1648`) then walks `stmt.cmdsub_bodies()`
   **unconditionally**, calling the full `check_stmt` on each inner statement,
   which reaches `check_command` — which also calls `check_keys_operand`.

Path 2's comment explains why it is unconditional:

> This is unconditional rather than part of the `_` arm because the arms that
> do NOT reach `check_stmt_bodies` — `Command`, `Declaration`, `Assignment` —
> are exactly the ones that carry a substitution.

That reasoning enumerates the leaf statement kinds and does not consider `For`,
which has its own handler that was already walking its header words. So the
two paths overlap for exactly one statement kind.

This is why the table above looks arbitrary: an assignment reaches path 2 only,
a for-header reaches both.

## Why it surfaced now

It did not surface before because `check_collection_reader_taint`'s `keys` arm
answered first with an SB5020, and that pass runs before `type_check`. Deleting
that arm (same commit as this filing) let the real diagnostic through — twice.

So this is a pre-existing defect that a fix exposed, not a regression.

## The fork — needs DG

The two walkers are **not** equivalent, so this cannot be fixed by deleting one.

| | `check_subscript_in_stmt` | `check_command` |
|---|---|---|
| `check_subscript_types(&c.name)` | yes | no |
| `check_join_operand` | yes | yes |
| `check_keys_operand` | yes | yes |
| `check_len_operand` | **yes** | **no** |
| `check_string_intrinsic_operands` | yes | yes |
| `check_int_sink_operands` | no | **yes** |
| fill-target registration | no | **yes** |
| args `check_subscript_types` | yes | yes |

Three options, no recommendation — the choice is about what the two walkers are
*for*, which is a structural call:

**(a) Make `check_for` skip `CommandSubstitution` words.** Smallest change,
keeps both walkers. Leaves the asymmetry in place, so the next statement kind
that grows a word-walker can reintroduce the same duplicate.

**(b) Merge: move `check_len_operand` and the name-subscript check into
`check_command`, delete the `CommandSubstitution` arm from
`check_subscript_types`.** Removes the overlap at its root. But it widens where
`check_len_operand` fires — today a bare `len` operand is checked only inside a
`$( )`, and after this a statement-position `len` would be checked too. That is
plausibly a latent bug of its own (why is `len`'s operand only checked in one
position?), but it is a behaviour change and may newly reject programs.

**(c) Deduplicate identical (code, span, message) diagnostics at the
collection point.** Fixes this and any future instance in one place, and is the
only option that needs no judgement about walker roles. But it hides structural
overlap rather than removing it, and would mask a genuine case where the same
rule should fire twice.

## Note

`types.sh` 75.128/75.129 assert `$err == *SB4030*` without a count, so they do
not pin the duplication either way. Whoever fixes this should tighten them to
`$(grep -c SB4030 <<< "$err") = 1`, the idiom 75.116 already uses.
