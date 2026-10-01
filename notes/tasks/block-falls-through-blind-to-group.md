---
status: done
priority: P3
resolved: 2026-10-02
depends:
gates:
---

# `block_falls_through` cannot see through a `{ … }` group

`validate.rs::block_falls_through` decides whether a branch reaches the merge
by inspecting the block's **last statement** and matching a literal
`exit` / `return` / `break` / `continue` command. A `Group` is not a command,
so a block that diverts inside braces is treated as falling through:

```sash
argv a
attest v in a [[ -n $v ]] || { exit 1; }
for v in @a; do echo "$v"; done     # SB5089 — but the path cannot reach here
```

The unbraced form compiles (spec `div.1`). The braced form is rejected, as
is the only way to say something before diverting:

```sash
attest v in a [[ -n $v ]] || { echo "bad input" >&2; exit 1; }
```

That second shape is the common one, and `{ … }` is the only multi-statement
`||` right operand the language has, so this closes off the idiom entirely.

## Resolved

DG raised it as a consistency question — *"why isn't `attest s g? || { exit
1; }` handled identically to `if ! attest s g? { exit 1 }`?"* — which is the
right frame: the two are the same program, so a difference is a defect, not a
missing nicety. Asking it that way also turned up more surface than the
filing had:

| spelling | before |
|---|---|
| `attest x … \|\| exit 1` | compiles |
| `attest x … \|\| { exit 1; }` | **false SB5089** |
| `if attest x …; then :; else { exit 1; }; fi` | **false SB5089** |
| `if ! attest x … { exit 1 }` | **SB5088** — a different bug, see below |

The `else { … }` case was not in the filing. Because `block_falls_through` is
shared between `merge_paths` and `taint_list`, one recursion into a trailing
`Group` fixed both — which is the payoff for having made it `pub(crate)` and
shared rather than copied.

Fixed; `spec/taint.sh` div.17-div.23, including the three guard rails that
must keep rejecting (`|| { echo no; }`, `else { :; }`, `&& { … }`) and the
runtime check that a braced divert really diverts.

Still syntactic beyond the group: a trailing `if` that diverts on every arm
reads as falling through. Widening that means deciding divergence for
arbitrary control flow, which is a different question and is not filed.

## Not fixed: `if ! attest …`

DG's other spelling is a **separate** defect with a separate cause, filed as
`notes/tasks/negated-attest-not-a-caught-form.md`. `condition_attest` matches
only `StmtKind::Command`, so a `StmtKind::Negated` wrapper means the `if` is
never recognised as a caught attest at all; the attest is then walked as a
nested body, `JustTainted` resets, and R1 reports SB5088 "not adjacent" for
an attest that is adjacent. It needs DG's call because it adds a spelling
DD#121 does not name.

## Original analysis

## Scope

This is conservative rather than unsound — it rejects legal programs, it does
not accept illegal ones — which is why it is P3 and not a P1. Both the `if`
path (`merge_paths`) and the list path (`taint_list`) consult
`block_falls_through`, so one fix covers both.

The fix is to recurse into a trailing `Group`, taking the group's own last
statement. Note the group does not scope (`notes/journal/453`), so there is
no scope interaction to reason about. Keep it syntactic: do not try to decide
whether a trailing `if` diverts on every arm, which is a different and much
larger question.

## Related

- `notes/tasks/bug-attest-merge-drops-diverted-path.md` — the scalar half of
  the same exemption, blocked on a design call.
- `spec/taint.sh` div.1-div.9 pin the unbraced forms.
