---
title: "Bug: `return` is not an int sink — a string operand silently returns 0 (a literal ICEs native), though DD#89 records `return $s` as SB4030"
status: open
priority: P1
kind: bug
depends: []
gates:
  - "rt1, rt2, rt3, rt4, rt5, r6, r7 and r8 are SB4030 in both backends, with no ICE, and the message names a return status (not `int_sink_role`'s fallback, \"a positional index\")"
  - "r4 `return -1` compiles and returns -1, as `return $(( -1 ))` does (r9) — met by F1+F4 (journal 537), with a2 and a9"
  - "a4, a5 and s2 (in `r4/`) are SB4030 with a return-status message, as `exit`'s `r4/e1` and `r4/e2` are, and not SB5011 \"command not found\""
  - "a6, a7, a10, a11 and `fname/r1` are read as DG's ruling in bug-function-name-shadows-command-path implies"
  - "the controls are unchanged: 74.55, 74.56's first half, rt10, r2, r3, 84.46 (SB5046)"
  - "74.56's second half (`return \"5\"`) and 74.57-74.61 follow DG's answer to the question below"
  - "native and C agree on a non-numeric literal in an int position: neither silently reads it as 0"
  - "a DD#89 addendum replaces the false `return $s` row; a spec pins each row in both modes"
---

## Found

2026-10-27, while porting F1+F4 (journal 537). It is **pre-existing**: every
row but r4 is the same at `8e28c61`, with F1+F4, and on `taint-walker-redesign`
(`e98fca8`), in both backends unless noted. F1+F4 fixed r4.

| Id | Program (`f` then `echo "[$?]"`) | Result |
|---|---|---|
| rt1 | `fn f { str s=abc; return $s }` | `[0]` |
| rt2 | `fn f { str s=5; return $s }` | `[5]` |
| r8 | `fn f { str s=7; return $s }` | `[7]` |
| rt3 | `fn f { return "abc" }` | native: ICE, `codegen/values.rs:1422` "non-integer literal `abc` reached int lowering; type_check (SB4030) should have rejected it"; C: `[0]` |
| rt4 | `fn g { echo abc }`, `fn f { return $(g) }` | `[0]` |
| rt5 | `fn g { echo abc }`, `fn f { str s=$(g); return $s }` | `[0]` |
| r6 | `fn f { str s=abc; return "x$s" }` | `[0]` |
| r7 | `fn f { return "" }` | `[0]` |
| r5 | `fn f { int n=3; return "$n" }` | `[3]` |
| rt8 | `fn f { return "5" }` | `[5]` (74.56 pins it) |
| r1 | `str s=abc`, `fn f { return ${s:-7} }` | `rc=7` (74.57 pins the shape) |
| r4 | `fn f { return -1 }` | SB5011 1:15 "command '-1' not found"; **`[-1]` since F1+F4** (see Numeric operands) |

`exit` with the same operands, for contrast: `exit $s` (rt7, e1),
`exit $(g)` (rt6), `exit "5"` (rt9), `exit "$n"` (e5), `exit ""` (e6),
`exit ${s:-7}` (e2) and `exit ${s:?not an int}` with `s=5` (e3) are all
SB4030; `wait ${s:-7}` (w1) too. `exit -1` (e7) runs and exits 255.
The accountable spellings work for both: `return $(( ${s:-7} ))` (r2)
returns 7, `return $(( ${s:?not an int} ))` (r3) aborts with the message,
`exit $(( ${s:-7} ))` (e4) exits 7. Controls: rt10 `int n=4; return $n`
prints `[4]`; r9 `return $(( -1 ))` prints `[-1]`.

Probes: `files/port-f1f4-probes.tar.gz`, directories `rt/`, `rt2/` and
`rt3/`.

### Numeric operands

F1+F4 (journal 537) made `parse_return` read a word as a status value exactly
when the int sink's literal test takes it (`ir::is_int_sink_literal`, an i64
`parse`). Before, only all-digit words were values. Each program below is
`fn f { return X }; f; echo "[$?]"`, with the same result in both backends
unless noted. These ids name files in `r4/`, so `r4/e1` is not the `e1` above.

| Id | X | `8e28c61` | with F1+F4 |
|---|---|---|---|
| a1 (= r4) | `-1` | SB5011 | `[-1]` |
| a2 | `+1` | SB5011 | `[1]` |
| a9 | `-9223372036854775808` | SB5011 | `[-9223372036854775808]` |
| a8 | `9223372036854775807` | `[9223372036854775807]` | the same |
| a4, s2 | `99999999999999999999`, `9223372036854775808` | native ICE (values.rs:1422 "non-integer literal … reached int lowering"); C compiles, then the program fails with "arithmetic overflow", rc 2 | SB5011 "command '…' not found — use absolute path or define as function" |
| a5 | `-99999999999999999999` | SB5011 | SB5011 |
| a6, a7, a10, a11 | `-x`, `--1`, `+-1`, `-1x` | SB5011 | SB5011 |

`exit` rejects the out-of-range and non-numeric operands with SB4030:
"`exit` takes an exit status, but `99999999999999999999` is not a decimal
integer" (`r4/e1`; `r4/e2` for `-99999999999999999999`, `r4/e3` for `-x`).
`exit +1` exits 1 (`r4/e4`).

So F1+F4 traded the a4/s2 ICE for an SB5011 that reads the numeral as a
command name. The value/producer split is now the i64 range. A function may be
named `99999999999999999999`, and then `return 99999999999999999999` calls it
(`fname/r1` in `bug-function-name-shadows-command-path`). `return 9` stays the
value 9 even when a function is named `9` (`fname/r2`).

Probes: `files/port-f1f4-probes.tar.gz`, directory `r4/` (a1-a12, s1-s3,
e1-e4; `after.txt` holds the F1+F4 results).

## DD#89 records a rejection that does not exist

DD#89's Resolution (2026-09-26) table has the row
`` `return $s` | atoi → 0 | **SB4030** | journal 411 ``, and journal 418
copies it. Journal 411 fixed the **disposition** forms only:
`handle_exit_status_assign` now lowers `return ${s:-7}` / `${s:?…}` through
`lower_value_as_int`, so they convert with the arithmetic trigger (74.57-74.61).
Its inventory table's "was broken, now fixed" is in the *accountable
spelling* column. Nothing ever made a bare `return $s` a type error, and
`a875d58` ("exit, wait and arg take an int, not a string") left `return`
out. The `undispositioned-string-to-int-sites` task (done) repeats the claim
("`return` (journal 411 …)" among the sites "closed first").

## Cause (read, not yet confirmed by a fix)

- `return VALUE` is a plain `return` Command (`parse_return_value_form`,
  parser.rs 1362-1368), so `check_command` → `check_int_sink_operands` sees
  it. But `int_sink_slots` (type_check.rs 1459-1464) is
  `"exit" | "wait" | "arg" => Some(1)`, so the operand is never checked.
  If `return` is added as-is, `int_sink_role` (1507-1513) falls through
  to `_ => "a positional index"`. It needs its own arm.
- Lowering: `handle_return_stmt` (transform/stmt_handlers.rs 434-452) passes
  the operand to `emit_return_value`. Native's `int_literal_or_zero_init`
  (codegen/values.rs 1417-1424) panics on a non-numeric literal, as its doc
  says it should once SB4030 has run. C's `lower_value_as_int`
  (emit_c.rs 1572-1580) sends the same literal to the lenient `sash_atoi`,
  so C prints `[0]`. A string variable goes through `sash_atoi` in both.
- r4: `parse_return` treated any `Word` that is not all digits as a producer
  head, so `-1` was looked up as a command. F1+F4 fixed that by giving the
  parser the int sink's own literal test, `text.parse::<i64>().is_ok()`.
  That test was inline in `operand_satisfies_int_sink`, and `a875d58` kept
  signed literals legal there ("`exit -1` exits 255, as in bash"). It is now
  shared as `ir::is_int_sink_literal`. What remains is a numeral beyond i64,
  which still heads a producer (a4, a5, s2).

## What is settled, and what is not

Settled:
- DD#89's Resolution is DG's ruling: `return $s` is SB4030 ("settled it the
  same way in every case: by rejection"). That covers rt1, rt2, rt4, rt5, r6
  and r8, whatever the string's value.
- `a875d58` records three DG calls for the int sink, and they settle the
  rest once `return` is one (decide-and-report): `''` is rejected (r7); a
  quoted number is a string (rt8, r5); a signed decimal literal stays legal
  (r4). 74.56 pins `return "5"` as a legal "quoted … status VALUE form", but
  it was added a day before `a875d58` (4578aed, 2026-09-25). The later call
  reasons from the whole language ("Quoting is the int->string conversion
  everywhere else in sash"), so 74.56's second half flips (keep the id, the
  F14 precedent).

Question for DG: **the disposition forms.** `return ${s:-7}` converts with
DD#77's arithmetic trigger (journal 411; 74.57-74.61): `abc` is not an
integer, so the default applies (r1). No other int position converts a
string-typed disposition outside `$(( ))`:
- `exit ${s:-7}` and `exit ${s:?m}` are SB4030 (e2, e3). That comes from
  `a875d58`'s predicate; neither its message nor any spec mentions a bare
  disposition operand (its message shows `exit $(( ${s:?bad} ))`).
- `int m=${s:-7}`, and `m=${s:-7}` on an int `m`, are SB4030 advising
  `$(( ${VAR:-0} ))` (x4, x6); `int m=$(( ${s:-7} ))` prints `[7]` (x5).

The options:
- (a) **One rule (recommended).** `return` joins the sink with the same
  predicate, so `return ${s:-7}` is SB4030, and 74.57-74.61 are re-pinned
  with the `$(( ))` spelling. Their assertions carry over (r2, r3). `return`
  then takes the spelling the other int sinks and int assignment take, which
  is DG's "prioritize sash self-consistency". Two consequences:
  - `return ${n:?m}` on an int `n` works today (x3, `[5]`). It would become
    the false SB4030 that `exit ${n:?m}` already is (x1), so
    `bug-expr-type-int-disposition` lands first.
  - The int-disposition arms (native `lower_value_as_int`'s, C's
    `lower_disposition_as_int_c`) may lose their last producer. Enumerate
    their producers, then apply the cold-arm rule.
- (b) **Loosen every int position.** `exit`/`wait`/`arg` and int assignment
  accept a string-typed disposition and convert it as `return` does. DD#89's
  amendment called `return ${s:?bad}` "the spelling SB4030 itself
  recommends"; today the sink's and the assignment checker's SB4030 advice
  both say `$(( … ))`.
- (c) **`return` only.** Reject the undispositioned forms but keep `return`'s
  dispositions. `return` stays the one exception.

## Fix direction

Add `return` to `int_sink_slots` with its own `int_sink_role` arm. Then widen
`parse_return`'s value test from "the sink takes it" to "it is numeral-shaped"
(an optional sign, then digits), so the sink rejects an out-of-range numeral
with SB4030 as it does for `exit` (a4, a5, s2). This cannot be done before
`return` is a sink: the out-of-range value would reach native int lowering and
ICE again, as it did at `8e28c61`. Whether a sign-led non-numeral (`-x`) heads
a producer or is a value turns on which function names are legal
(`bug-function-name-shadows-command-path`). Make C refuse a non-numeric
literal the way native does, through one shared helper. Check first whether
`SsaValue::Expr` reaches that arm with non-numeric text. Then write a DD#89
addendum that corrects the row and records the disposition ruling, and a spec
per row in both modes.
