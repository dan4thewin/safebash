title: "SB5037: a mutual-recursion cycle is reported once or twice, depending on definition order"
status: open
priority: P4
kind: bug
depends: []
gates:
  - "DG picks R1 or R2 (no DD settles the count)"
  - "the bf24 family reports the same count in both definition orders, both backends"

## What

A recursion cycle draws SB5037 once or twice depending on which function is
defined first and whether the calls are stages. Each report is a correct
witness; the count is the defect. Found during F2 (bf24, journal 539).

Probes in files/port-f2-probes.tar.gz (`pr/`):

- bf24 = `fn f { g | /bin/cat }` ⏎ `fn g { f | /bin/cat }` ⏎ `f`
- bf24p = `fn f { g }` ⏎ `fn g { f }` ⏎ `f`

| Probe | Shape | base | branch | after F2 |
|---|---|---|---|---|
| bf24 | stage, f then g | 1 (`f`) | 1 | 2 (`g`, `f`) |
| bf24s | stage, g then f | 2 | 2 | 1 |
| bf24n | stage, no call | 1 | 1 | 1 |
| bf24p | plain, f then g | 1:1 `f` | 2:8 `f` | 1:8 `g` + 2:8 `f` |
| bf24ps | plain, g then f | `g` + `f` | 2:8 `g` + 1:8 `f` | 1:8 `f` |
| bf24m | stage and plain mixed | - | - | same as bf24p |

C and native agree everywhere.

## Mechanism

- SB5037 has one emitter, `functions::recursion_diagnostic`.
- `lower_function_stmt` lowers a `fn_` template that sees only the functions
  defined so far, so a forward call lowers as an external. The call site
  inlines again and finds the cycle from the other end.
- `ir_to_ssa_typed`'s dedup is by exact text, so two different witnesses
  both survive.
- F13's c35 fix pushes an `inlining` frame in templates, which flipped which
  definition order double-reports.

## Fix options

- R1, one report per cycle: key the dedup by the cycle's member set. This
  also collapses two distinct recursive sites in one body:
  `fn f { f; echo a; f; echo b }` reports 1:8 and 1:19 today.
- R2, one report per recursive call: templates see every definition (scoped
  to templates; DD81 keeps a top-level call before its definition external),
  so the call-site report duplicates the template's exactly and the dedup
  removes it.
