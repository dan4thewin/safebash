status: done
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


## Done — and the filing above is wrong in two ways

DG, reading the three options: *"once you find an error in part of the tree,
why walk that portion again? seems like that is a recipe for numerous
errors."*

That reframed it, and measuring the reframing broke the filing.

### It was never specific to a for-in header

`keys` is the only intrinsic validate permits in a for-header `$( )` and
rejects almost everywhere else, so every shape I compared it against was
rejected one pass earlier. The duplication is **general to a command
argument**:

| shape | HEAD | fixed |
|---|---|---|
| `for k in $(keys a) { … }` | 2 | 1 |
| `echo "$(join s X)"` | **2** | 1 |
| `echo "$(join s X)" "$(join s X)"` | **4** | 2 |
| `f "$(join s X)"` (user fn) | **2** | 1 |

### There was no fork between two legitimate walkers

`check_subscript_in_stmt` was a **private fork of `check_command`'s
operand-check tail** that also descended into `$( … )` — the journal 441 /
456 pattern. `check_stmt` already walks `cmdsub_bodies()` unconditionally and
runs the *full* check on each inner statement, so the fork's descent was pure
duplication. It was deleted, and its two unique checks
(`check_subscript_types(&c.name)`, `check_len_operand`) moved into
`check_command`.

Option (a) would have patched one caller, (b) feared a behaviour change that
measurement showed was a bug fix, and (c) would have hidden the redundant work
rather than removed it. None of the three was right.

### The fork was hiding a live hole

The two copies had drifted in **opposite** directions: `check_len_operand`
only in the fork, `check_int_sink_operands` only in the original. So
`len`-on-a-scalar was caught in the positions the fork reached and nowhere
else, and `dd75-len-collection-only-strlen` is `status: done` with the gate
*"`len <scalar>` → compile error naming `strlen`"*:

```sh
str s="ab"
str j="$(len s)"   # accepted; j was "2" — the exact overload DD#75 removed
int n=$(len s)     # accepted
echo "$(len s)"    # SB4030, correctly
```

### And a second hole of the same shape, NOT closed here

An assignment and a declaration walk their *target's* index and never their
**value**, so `${a[foo]}` on an indexed array is a clean SB4030 as
`echo "${a[foo]}"` and an **ICE** as `str v="${a[foo]}"` —
*"non-integer literal `foo` reached int lowering; type_check (SB4030) should
have rejected it"*. Pre-existing; confirmed against a pristine HEAD build in a
scratch worktree.

The one-line fix was written and **reverted**: walking the value regresses
`functions.sh` lift.3, which pins `str z=${arr[$(f)]}` as *accepted* while the
same expression in an argument is SB4030. Closing the ICE therefore requires
ruling on whether DD#80 permits a capture as an index, which is DG's call.
Filed as `capture-as-subscript-index-position-dependent` (P1) with the
evidence and three options.

### Evidence

66-program differential against a pristine HEAD binary: **0 diagnostics lost**,
12 programs deduplicated, 1 newly (correctly) rejected. Specs `types.sh`
75.148-75.156, and 75.128/75.129 tightened from
`== *SB4030*` to `$(grep -c …) = 1` as this filing asked.

### DG's wider question, left open

Removing the overlap removed every duplicate in the corpus, so no
suppress-after-error guard was needed *here*. Whether to add one anyway — so
that a future overlap cannot produce duplicates — is still open. Filed as
`suppress-rewalk-of-errored-subtree`.
