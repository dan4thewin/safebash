---
status: open
priority: P3
---

# Purge the `PreIncDec` dead node (or decide it stays)

`ArithExpr::PreIncDec` is constructed by `parse_pre_incdec`
(`sash/src/parse/arith.rs:1116`) and then **always rejected before it
leaves the parser**. It exists only so `reject_prefix_incdec` can name
the offending variable in its diagnostic.

## Evidence

`reject_prefix_incdec` is called at both arithmetic entry points, ahead
of every other check:

- `parse_arith_expansion:78` — `$(( … ))`
- `parse_arith_slot:280` — `(( … ))` and all three C-style `for` slots

and it recurses through `BinOp`, `Assign`, `CompoundAssign`, `Comma`
and `Paren` — the same node set as `has_mutation_inner`, so nothing can
hide a `PreIncDec` below it.

The two `parse_arith_str` callers that bypass `parse_arith_slot`
(`expr.rs::emit_string_part:508`, `arith.rs::parse_expansion_atom:1048`)
carry their own rejection. Probed:

| input | result |
|---|---|
| `echo $(( ++y ))` | SB4020 prefix |
| `echo $(( 1 + ++y ))` | SB4020 prefix |
| `(( ++y ))` | SB4020 prefix |
| `(( y = 1 + ++y ))` | SB4020 prefix |
| `echo $(( (++y) ))` | SB4020 prefix |
| `echo "$(( ++y ))"` | SB4020 (string path) |
| `echo "${y:-$(( ++y ))}"` | SB4020 (string path) |
| `for (( i=0; i<2; ++i ))` | SB4020 prefix |

## The unreachable handling

- `parse/arith.rs:189` — `brief_arith`'s `PreIncDec` arm (2 cold
  branches; this is what surfaced it)
- `parse/arith.rs:1292`, `ir.rs:1133`, `validate.rs:2033`,
  `validate.rs:3442` — shared `PostIncDec | PreIncDec` patterns where
  only the `PostIncDec` alternative can fire
- `transform/statements.rs:470`, `transform/expr_handlers.rs:376`
- `emit_ir.rs:648`

## The question for DG

Deleting the arms is mechanical and git makes it reversible, but it
spans four passes, and a parse-only IR node keeping defensive
downstream arms is a defensible position: if a future change ever lets
one through, an `unreachable!()` is a better outcome than a silently
wrong lowering.

Three options:

1. **Delete** every `PreIncDec` arm downstream of the parser.
2. **Convert** them to `unreachable!()` with the invariant named — the
   `join_intrinsic` convention already in the tree.
3. **Leave** them and accept ~10 permanently cold branches.

Option 2 keeps 100% branch coverage honest (an `unreachable!()` is a
should-never-run assertion, the kind `mk cov-unit` is deliberately off
to avoid hiding) while documenting *why* the code cannot run.

Found in journal 338.
