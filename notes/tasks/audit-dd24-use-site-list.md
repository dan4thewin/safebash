title: "Audit DD#24's use-site list against the code, entry by entry"
status: open
priority: P1
kind: audit
depends: []
gates:
  - "every entry in DD#24's use-site list has a spec pinning it in every position the 2026-08-26 amendment names (argument, assignment RHS, `[[ ]]` operand, subscript index, array-literal element, redirect target, arithmetic operand, function argument, nested cmdsub)"
  - "any entry found unimplemented is filed as its own bug and fixed under the Bug policy, not folded into this task"

## Why

DD#114 (bug #13) found that a tainted **subscript index** was not treated as a
read. DD#24 has always listed it — "`arr[$x]=v` — index expression" — and its
2026-08-26 amendment states the list is **position-uniform**. Three taint
walkers matched `Subscript { name, .. }` and dropped the index; two positions
(`s="${a[$i]}"`, `[[ -n ${a[$i]} ]]`) were accepted outright, which is a
laundering bypass.

Zero specs used a tainted subscript index, so the sweep was identical before
and after the fix. **The surface was uncovered, so nothing disagreed with the
code.** The same can be true of any other entry.

This is the sixth instance of "authoritative text describes a class, code
handles the instances someone tried", and the first where the text is a
decision document rather than a code comment. The other five were found one at
a time, by accident, over a month. This task is the attempt to stop doing that.

## What to audit

The cross product of DD#24's use-site list with the amendment's position list.
Each cell is a spec.

Entries (DD#24 "Tainted data cannot be used at ALL use sites"):

- `$x`, `${x}`, `${x:op}` — bare and operator expansions
- `${x[i]}` — container read *(container name: done; index: DD#114)*
- `b=$a` — assignment RHS read
- `cmd "$x"` — command argument
- `arr[$x]=v` — index expression *(DD#114)*
- `(( x ))` / `$(( x ))` — arithmetic operand
- passing as a function argument
- `join @arr` when any element is tainted

Positions (2026-08-26 amendment — "position-uniform"):

- command argument (external and builtin — they are different code paths)
- assignment RHS
- subscript index
- array-literal / map-literal element
- `[[ … ]]` test operand
- redirect target
- arithmetic operand
- nested inside a `$( )`

## Method

For each cell, **write the program and run it** — do not read the walker and
conclude. Bug #13's three walkers all *looked* right; `..` was the whole bug.
Two failure signatures to watch for:

1. **accepted** — a laundering bypass, P0.
2. **rejected by the SSA taint backstop** — the message has no SB code, no
   line:col, and an SSA-internal name (`i_1`). `validate` is the authoritative
   gate and the backstop is defense-in-depth
   (`notes/tasks/ssa-taint-full-backstop.md`); reaching it at all means a gap
   in the gate. This is the signature that found bug #13.

A cheap first pass: `grep` the taint walkers for `{ name, .. }` and any other
pattern that binds part of a node and discards the rest.

## Scope note

Findings are bugs under the Bug policy — file and fix each one on its own,
with its own DD if it settles a question. Do not batch them into one commit
"closing the audit".
