# 335 — the ghost operator

Branch coverage re-measured after the purge: **88.34%, 431 cold** (was 88.17% /
443). Then 18 new specs and a second purge. Sweep **5139 passing, 10 skipped,
0 failing** — predicted exactly (18 specs × 2 modes = +36).

## Mining found a ghost

The densest cold function was `detect_array_iter` — the for-loop iterable
classifier — with 8 cold branches. Three of its arms:

```rust
ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => …
ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrkeys") => …
ExprKind::DoubleQuoted(parts) if parts.len() == 1 => detect_array_iter(&parts[0].kind),
```

The branch records said `574|0|0 = 160, 574|0|1 = 0`: the `arrvals` guard
matched 160 times and was **never false**. Every parameter expansion ever
iterated was an `arrvals` splat, so the `arrkeys` arm below it never got a turn.

That could mean "no spec iterates a map's keys" — except `for k in $(keys m)`
is specced three times over. So the keys path exists and does *not* go through
`arrkeys`. Where does `arrkeys` come from?

Nowhere. That is the finding.

```
$ grep -rn 'Some("arr[a-z_]*")' sash/src | grep -v 'as_deref\|matches!\|=='
parse/expr.rs:220   Expr::param(&w[1..], Some("arrvals"), …)
parse/expr.rs:411   Expr::param(name, Some("arrvals_retired"), …)
parse/expr.rs:553   Expr::param(name, Some("arrvals_retired"), …)
parse/expr.rs:812   Expr::param(name, Some("arrkeys_retired"), …)
parse/expr.rs:1057  Expr::param(name, Some("arrvals_retired"), …)
parse/expr.rs:1414  operator: Some(format!("arrvals_{}", op_str))
transform/pipelines.rs:74     Some("arrlen")
transform/expressions.rs:195  "arrlen"
transform/expr_handlers.rs:330 "arrjoin"
```

`arrvals`, `arrvals_*`, `arrlen`, `arrjoin`, and the two `*_retired` forms. No
`arrkeys`. Yet six sites across four passes handled it: `validate.rs` twice,
`transform/control_flow.rs`, `codegen/value_handlers.rs` twice, `emit_c.rs`.

## Where the ghost came from

DD#75 §3 made `$(keys m)` the canonical key splat and retired `${!a[@]}`.
`arrkeys` was `${!a[@]}`'s operator. When the spelling was retired, the parser
started emitting `arrkeys_retired` instead — which `retired_bracket_operator_diag`
rejects at parse with SB4038 — and every downstream handler for the old name
was orphaned in place.

`$(keys m)` never produces an operator at all. It is a command substitution, and
`detect_array_iter` classifies it structurally, in the arm four lines below:

```rust
ExprKind::CommandSubstitution(stmts) if stmts.len() == 1 => {
    …
    if cmd_name != "keys" { return None; }
    c.args.first().map(|a| (expr_to_string(a), true))
}
```

Same `true`, different route. The two mechanisms coexisted, one of them unused,
because retiring a surface deleted the *producer* and left the *consumers*.

This is the cohort `remove-retired-surface-implementations` already covers, and
DG's ruling there is directly on point: *"Retired surfaces should not keep
working implementations. The validator refuses them; the code behind them is
unreachable from any program that compiles, and it is live enough to pass
tests, which makes it look supported."*

## Measuring the blast radius before cutting

Last session's lesson was that my first reason for a deletion can be wrong, so
this time I traced every consequence before touching anything.

- `codegen/value_handlers.rs:124` picked `externs.table_key` over
  `externs.table_value` for `arrkeys`. Tempting to conclude the `table_key`
  extern and `sash_table_key` in the runtime die with it — **they don't**.
  `codegen/dispatch.rs:615` uses `table_key` for the real `$(keys m)` loop. The
  extern stays; only the selector goes.
- `emit_c.rs::emit_arr_keys_joined` had exactly one caller, the dead arm. Dead.
- Its callee `emit_arr_join_loop(e, table_expr, sep, keys: bool)` then has one
  caller with `keys = false`, so the parameter is a constant wearing a
  parameter's clothes — the same shape the earlier task found in `ReadLine.delim`.

Coverage confirmed the last one independently, which is the check I wanted:

```
4048|0|0|0      ← if keys { "sash_table_key" }
4048|0|1|33     ← else     { "sash_table_value" }
```

Thirty-three joins emitted, not one of them keys.

## What was removed

| pass | removed |
|---|---|
| `validate.rs` | `Some("arrkeys")` from `is_arrvals_splat_op` and `splat_array_name` |
| `transform/control_flow.rs` | the `arrkeys` arm of `detect_array_iter` |
| `codegen/value_handlers.rs` | `op == "arrkeys"` from the join dispatch; the `table_key`/`table_value` selector collapses to `table_value` |
| `emit_c.rs` | the `"arrkeys"` arm, `emit_arr_keys_joined`, and `emit_arr_join_loop`'s `keys` parameter |

Five doc comments that listed `arrkeys` among the live operators were corrected
rather than left to mislead the next reader — including `parse/expr.rs`'s
expansion table, which still claimed `${!var[@]} → arrkeys` when it has produced
`arrkeys_retired` since DD#75.

Verified after: `for k in $(keys m)` still prints `k=x / k=y`, `$(join m)` still
gives `1 2`, `$(join a)` still gives `p q`, in both backends.

## 18 specs

Coverage-driven, `cov.223`–`cov.240`, every one probe-verified before it was
written.

Six exist only to drive a lexer path. `scan_braceless_operand` handles the
operand of a brace-less `@{a:-…}`, and its `$…` scan had never seen anything but
a one-character identifier:

| spec | input | what it drives |
|---|---|---|
| cov.223 | `@{a:-$xy}` | a multi-char name — the inner run loop actually advances |
| cov.224 | `@{a:-$_z}` | an `_`-led name — the second alternative, which an alphabetic lead short-circuits past |
| cov.225 | `@{a:-$#}` | a single-char special, consumed *before* the `#` word-terminator check |

`cov.225` is the interesting one: `#` is a word terminator in that scanner, so
whether `@{a:-$#}` is an operand of `$#` or an empty operand followed by a
comment depends on the scan order. It is the former.

Three more drive `detect_array_iter`'s newly-interesting non-splat paths:
`for x in "$v"` (quoted, one part — unwrap and re-test), `for x in "$v$w"`
(quoted, two parts — not unwrappable, the concatenation is the item, prints
`ab`), and `for x in ${v:-d}` (an expansion with a non-splat operator, which
reaches both splat tests and fails both).

The rest pin one-line rejections no feature spec bothers to write: `$0` → SB5017
(*not* the function-local family), `$@`/`$*` → SB5045 with **different
directives at top level and inside a function**, `$1`/`$#` → SB5024 likewise,
`foo/bar` and `./x` → SB5051, `echo*` → SB5011. Plus three accepted subscript
spellings: `${m["k"]}`, `${m['k']}`, and `${a[$((1))]}` → `y`, which pins that
an arithmetic index is evaluated rather than used as a literal key.

## A trap worth naming

`echo*` reports *"command 'echo*' not found"*, and `is_valid_command` contains

```rust
if name.contains('*') { return true; }   // Glob pattern — not a command
```

which says a `*` head is **valid**. For a while that looked like a bug. It is
not: the identical SB5011 message is emitted from **two different passes**, and
this one comes from `resolve.rs:244`, not `validate.rs:2887`. The `*` arm is the
same deliberate deferral as the `/` arm right above it — let `resolve` own the
diagnostic so every position reports the same thing.

Two passes emitting a byte-identical diagnostic makes a trace ambiguous from the
outside. Not worth changing the wording, but worth knowing.

## Reported, not acted on

`arrjoin_separator`'s branch records read `4036|0|0 = 33, 4036|0|1 = 0` — it is
*only* ever called with `operator == "arrjoin"`, so the `arrvals` path never
reaches the C backend's join at all, despite `emit_c.rs:2820` matching
`"arrvals" | "arrjoin"`. That is a second possible ghost, and it wants the same
trace-before-cutting treatment rather than a guess at the end of a session.
Filed as a lead, not touched.
