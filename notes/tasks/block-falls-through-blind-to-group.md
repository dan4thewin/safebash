---
status: open
priority: P3
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
