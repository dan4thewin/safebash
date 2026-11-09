---
title: "Bug: a declaration or a `for` binds any word as a variable name (`str a-b=1`, `int /bin/x=1`, `str 9z=1`, `int a[0]=5`)"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "every program in the first table is a compile error at the name, naming it and the rule a name follows, in both backends; none binds a variable"
  - "the controls keep their results"
  - "a spec pins each rejected shape in both backends; each fails on the pre-fix binary"
---

## Found

2026-10-27, while porting F1+F4 (journal 537). Step 7 (B28) changed where a
declaration's assignment list stops, and probing that list showed its names
are never checked. Pre-existing at `8e28c61`, unchanged by F1+F4, and the
same on `taint-walker-redesign` (`e98fca8`); both backends.

| Program | Result |
|---|---|
| `str a-b=1` | SB5050 "variable `a-b` is assigned but never read" |
| `int /bin/x=1` | SB5050 `/bin/x` |
| `str 9z=1` / `const 9z=1` | SB5050 `9z` |
| `array /x=(1)` | SB5050 `/x` |
| `str s=1 /x=2` then `echo "$s"` | SB5050 `/x` |
| `str x+y=1` | SB5050 `x+y` |
| `str é=1` | SB5050 `é` (the read `${é}` is SB4047) |
| `array a=(1 2)`; `int a[0]=5`; `echo "${a[0]}"` | SB5050 `a[0]`: a scalar named `a[0]`, not the element |
| `map m=()`; `str m[k]=v`; `echo "${m[k]}"` | SB5050 `m[k]`, the same |
| `for a-b in x { echo hi }` | SB5050 `a-b` |
| `for a[0] in x { echo hi }` | SB5050 `a[0]` |
| `int n=1 /bin/echo hi` then `echo "$n"` | SB5050 `/bin/echo`: the command words are declared as variables |
| `str 9z=1` then `echo "${9z}"` | prints `1` |
| `int 9z=1` then `echo "$(( ${9z} + 1 ))"` | prints `2` |

Most rows fail only because the dead-variable check happens to fire, and
it reports the wrong fault. The last two compile and run: a declaration
binds a name that plain assignment rejects.

Controls observed in the 2026-10-27 probes (not remeasured here):

| Program | Result |
|---|---|
| `a-b=1` / `9z=1` | SB5011 "`a-b` is not a valid variable name, so this is neither an assignment nor a runnable command — a name is letters, digits and `_`, and cannot start with a digit" |
| `/bin/echo x \| readline a-b` | SB5057 |
| `keys m a-b` | SB5063 |
| `int in=3` | SB5092 (DD#125) |
| `str "x"=1` / `str $y=1` | SB4014 "expected variable name in declaration" |
| `int n=1 m=2` then `echo "$n$m"` | `12` |
| `int a=1 b=2 c`; `c=3`; `echo "$a$b$c"` | `123` |
| `str _x1=ok` | binds `_x1` |
| `str x="a"b=1` | `x` is `ab=1` |
| `s=x /bin/echo hi` then `echo "$s"` | ran the command; now a required separator refusal (DD71, 2026-11-09) |

Probes: `files/port-f1f4-probes.tar.gz`, directories `sq2/`, `sq3/`, `sq4/`.

## Why it is a bug

The validator states the rule for a name in SB5011's message, and every
other binding site enforces it: plain assignment (SB5011), `readline`'s
target (SB5057), a fill target (SB5063). Only a declaration and a `for`
variable skip it. `int a[0]=5` reads as an element write and silently is
not one. `int n=1 /bin/echo hi` reads as a prefix assignment and silently
declares two more variables (`q-statement-form-residuals` Q2, its d2).

## Cause

`expect_word` (`sash/src/parse/parser.rs` 4027-4050) returns any
`Tok::Word`, plus eight keywords, without checking the spelling. It reads
the name in `parse_one_decl_assignment` (1187) and the variable in
`parse_for` (875). A quoted or `$` word is a different token, which is the
only shape it rejects (SB4014). The two other users of the call are covered
below: function names (`parse_function`, `parse_paren_function`) and attest
targets.

Plain assignment never reaches the parser as an assignment: `a-b=1` becomes
a command with a malformed head, and `bad_command_head_diag`
(`sash/src/validate.rs` 4170-4191) with `malformed_assignment_target`
(4200-4215) reports SB5011.

There are two identical identifier predicates: `ir::is_plain_identifier`
(`sash/src/ir.rs` 678-685, the validator's: `classify_read`, the fill and
`readline` target checks) and `parse::expr::is_identifier`
(`sash/src/parse/expr.rs` 456-460, four parser callers).

## Fix direction

- Check the name in `parse_one_decl_assignment` and `parse_for` with one
  identifier predicate, and reject at the name with SB5011's wording: the
  name, then "a name is letters, digits and `_`, and cannot start with a
  digit". Keep the keyword arm; DD#125 already rejects those names.
- A subscripted name can say that a declaration names a whole variable and
  an element is written as `a[0]=5`.
- A path can say that a declaration does not run a command
  (`int n=1; /bin/echo hi`).
- Make one of the two identifier predicates call the other, or delete one.
- `bug-digit-led-names` assumes nothing can bind a digit-led name, which is
  false until this lands. Either can land first.

DD71's 2026-11-09 policy makes plain and declared binding lists obey the
same separator rule; `q-statement-form-residuals` owns that enforcement.
Keep this name guard independent: an explicit separator does not make
`str 9z=1` a legal declaration, and a path-name refusal alone does not
prove that a legal untyped binding followed by a command requires a
separator. Valid multi-name declarations retain their syntax and types.

## Function names

DG settled function-name spelling on 2026-11-09: the same ASCII identifier
grammar as variables, plus existing reserved-name rules (DD125).
`bug-function-name-shadows-command-path` owns that enforcement. It found
what an unchecked name costs: a path-named function was checked as the
external program but ran as the function. These historical probes were
accepted too; their definitions must now reject under the chosen policy:

| Program | Result |
|---|---|
| `fn a-b { echo hi }` then `a-b` | prints `hi` |
| `fn 9z { echo hi }` then `9z` | prints `hi` |
| `fn a[0] { echo hi }` | accepted |

Both fixes should use one identifier predicate. Keep their guard evidence
independent: rejecting a malformed function definition does not prove that
a declaration or `for` target is checked. Neither implementation is
complete merely because the shared spelling rule is settled.

## Not this task

An attest target with a bad name binds nothing.
`attest a-b [[ $x =~ ^/ ]]` is SB5020 "attest condition must validate
$a-b — condition doesn't reference the variable", and
`attest x (( 9z = x ))` is SB4020. The SB5020 wording is odd, but no name
is created.
