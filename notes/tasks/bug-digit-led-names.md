---
title: "Bug: a name that starts with a digit (`${1x}`, `${1_}`, `$1x` in arithmetic) reads a variable nothing can bind"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "every program in the first table below is a compile error that names the malformed name, in both backends: none prints, none ICEs, none fails in gcc"
  - "in a word, `$1x` stays `$1` followed by `x` and `$12` stays `$1` followed by `2` (bash's rule; the controls below are unchanged)"
  - "the existing neighbours keep their diagnostics: `${?}` `${-}` `${#}` `${/a/b}` SB4034, `${$}` SB4047, `${1[0]}` `${[0]}` `${12[0]}` SB4047 (vm.un7-vm.un10)"
  - "a spec pins each rejected shape in both backends; each fails on the pre-fix binary"
---

## What

Each program is the body of `fn f { … }`, called as shown, unless it says
top level. The results are the same in both backends unless a column says
otherwise.

| Program | Call | Result |
|---|---|---|
| `echo "[${1x}]"` | `f` | `[]` |
| `echo "[${1_}]"` | `f` | `[]` |
| `echo "[${1x:-d}]"` | `f` | `[d]` |
| `echo "[${1x:?gone}]"` | `f` | aborts at runtime: `gone` |
| `echo "[${1x:0:1}]"` | `f` | native: compiler ICE "no codegen slot for `1x_0`; setup::map_inst_versioned must map every SSA version that defines a variable"; C: `[]` |
| `echo "[${1x:s/a/b/}]"` | `f` | native `[]`; C: gcc fails, "‘p1x_ptr’ undeclared" (rc 126) |
| `echo a ${1x:~} b` | `f` | native `a b`; C: gcc fails, "‘p1x_len’ undeclared" (rc 126) |
| `echo "[${1x}]"` | `f a` | SB5043 "function 'f' takes exactly 0 arguments but was called with 1" |
| `echo "[$(( $1x + 1 ))]"` | `f` | `[1]` |
| `echo "[$(( $12 + 0 ))]"` | `f` | `[0]` |
| `echo "[$(( $12 + 0 ))]"` | `f a` | SB5043, 0 arguments |
| `echo "[$(( $10 + 1 ))]"` | `f 4` | SB5043, 0 arguments |
| top level `echo "[${1x}]"` | | `[]` |
| top level `echo "[$(( ${1x} + 1 ))]"` | | `[1]` |

Each one reads a variable called `1x`, `1_`, `12` or `10`. That is not an
identifier, so no plain assignment binds it and the typo check never reports
it.

A declaration does bind such a name, until `bug-declaration-names-unvalidated`
lands. Re-run 2026-10-27 during F1+F4 (journal 537); the same on the current
tree and on 8e28c61, in both backends:

| Program | Result |
|---|---|
| `str 9z=1` then `echo "${9z}"` | prints `1` |
| `int 9z=1` then `echo "$(( ${9z} + 1 ))"` | prints `2` |
| `str 9z=1` or `const 9z=1`, then `echo hi` | SB5050 `9z`: the dead-variable check fires, and no name check does |
| `9z=1` then `echo "${9z}"` | SB5011 "`9z` is not a valid variable name …" |

The Fix below rejects the reads (`${9z}` is digit-led and not all digits), and
that task rejects the declarations. Either can land first, and each makes the
first two rows errors on its own. Probes: `files/port-f1f4-probes.tar.gz`,
directories `sq2/` (m6, n3, m4) and `sq3/` (m6a, m6b).

Bash: with `$1` = 4, `$(( $12 + 0 ))` is 42 and `$(( $10 + 1 ))` is 41,
because bash expands `$12` textually as `${1}2` before it evaluates. Sash
arithmetic reads an operand as a value and cannot append a digit to it.
For sash, an error is the only answer that neither differs from bash in
silence nor adds textual expansion to arithmetic.

Controls, correct today and to be kept: in a word an unbraced `$` takes
one digit, as in bash. `fn f { echo "[$1x]" }; f a` prints `[ax]`, and
`fn f { echo "[$12]" }; f a` prints `[a2]`.

## Cause

Three scanners take a digit-led name:

- `scan_param_name` (`sash/src/parse/expr.rs`) takes any `[A-Za-z0-9_]` run
  after `${`, so `1x`, `1_` and `10` become names. Its neighbour
  `bad_subscript_name_diag` already rejects the subscripted shapes
  (`${1[0]}`, vm.un7-vm.un10).
- In `sash/src/parse/arith.rs`, `scan_arith_dollar` sends an alphanumeric
  after `$` to `scan_arith_named_var`, which takes the whole `[A-Za-z0-9_]`
  run (`$1x`, `$12`). `braced_body_token` accepts a braced body through
  `is_bare_ident`, which is any non-empty `[A-Za-z0-9_]` run.
- `classify_read` (`sash/src/validate.rs`) returns `ReadVerdict::Silent` for
  any name that is not `is_plain_identifier`, so SB5031 never fires.

The positional readers do not claim these names either.
`positional_arg_index` (`sash/src/transform/expressions.rs`) accepts exactly
one byte `1`-`9`. `positional_index_1_9` (`sash/src/transform/functions.rs`)
parses the whole name as a number. Neither counts `1x` or `12` toward the
function's arity. The read therefore becomes a variable that is never bound:
- empty in a word;
- 0 inside an arithmetic expression;
- empty as a lone arithmetic operand (see
  `bug-reserved-names-in-arithmetic`);
- missing from the native slot map and the C declarations for the operator
  forms, which gives the ICE and the gcc failures.

## Fix (proposed, not yet ruled)

- After `scan_param_name`, a name that starts with a digit and is not all
  digits is SB4047 at the `${`, naming the name. For argument 1 followed by
  text, point the user at `${1}x`.
- `braced_body_token` accepts only an identifier or a single digit.
- In the arithmetic lexer, `$` and a digit followed by `[A-Za-z0-9_]` is an
  error of its own. The message says that bash would read it as `$1`
  followed by text, which sash arithmetic does not do.
- All-digit multi-digit names (`${10}`, `${01}`) are left to
  `bug-multi-digit-positionals`. That task decides whether they read an
  argument or are rejected.

Found during the batch-531 port (step F14, finding B38). Filed under DG's
rule for the port that questions and uncovered cases become backlog tasks.
