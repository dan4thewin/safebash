---
title: "BUG: the extent of a `${…:s/…}` expansion is decided by five scanners, and only the statement form's follows DD#63 and DD#96"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "one chain grammar, the statement form's (DD#63 Path B and DD#96), decides where every `${…:s/…}` payload ends: scalar, cell and positional; bare, inside `\"…\"`, inside a `$( )` within `\"…\"`, inside `$(( ))`, and inside a subscript"
  - "an unclosed or malformed payload is SB4032 at its `${`, in every form; none compiles"
  - "every row of the What tables reaches its expected result in both backends, and every row listed as already correct is unchanged"
  - "the `s_malformed` sentinel is gone (see Fix)"
  - "DG rules on the `$'…'` question below, and the ruling is recorded as a DD#63 addendum"
---

## What

DD#63 Path B scans the `:s/RE/SUB/[flags]` payload "as three `/`-delimited
fields, so a bare `{`/`}` inside RE or SUB is literal and does **not** close
the expansion", and "a **malformed** payload — fewer than the required
delimiters, or unterminated before the close — is a **compile error**, not
a silent fallback". DD#96 chains edits with `,` "in both spellings", and a
chain "may cross whitespace **and a line break**" while an edit is owed.
DD#65 (2026-07-04) makes quotes ordinary bytes inside `${…}`.

The statement form `x =~ s/…/` follows all three. The expansion form does
not, in five ways (B62–B65, B67, B68). Results were measured after F15 and
are the same in both backends. Every row behaves the same on the base commit
1309385 and on the batch-531 branch; t8, below, is the one program F15
changed.

### Should work and does not

| Id | Program | Today | Expected | Finding |
|---|---|---|---|---|
| n1 | `str x=abc` / `echo ${x:s/b/c/,` / `  s/c/d/}` | SB4072 at 3:9 | `adc` | B64 |
| n2 | the same, inside `"…"` | SB4032 at 2:6 | `adc` | B64 |
| c1 | `str x='a}b'` / `echo "${x:s/a/c/,s/}/d/}"` | SB4032 at 2:6 | `cdb` | B63 |
| c2 | the same, unquoted | SB4072 at 2:23 | `cdb` | B63 |
| q1 | `str x='a"b'` / `echo ${x:s/a"b/c/}` | SB4057 at 2:13 | `c` | B65 |
| q2 | the same, inside `"…"` | SB4057 at 2:20 | `c` | B65 |
| q3 | `str x='a"b'` / `echo ${x:s/b/c/,s/"/q/}` | SB4057 at 2:19 | `aqc` | B65 |
| q4 | `str x='a"b'` / `echo ${x:s/$'"'/X/}` | SB4057 at 2:14 | `aXb` | B65 |
| c16 | `str d='"'` / `echo ${d:s/"/Q/}` | SB4057 at 2:12 | `Q` | B65 |
| k1 | `array a=('a}b')` / `echo "${a[0]:s/}/x/}"` | SB4032 at 2:6 | `axb` | B67 |
| c13 | `array c=('a}b')` / `echo ${c[0]:s/}/X/}` | SB4072 at 2:19 | `aXb` | B67 |
| p2 | `fn f { echo ${1:s/}/x/} }` / `f 'a}b'` | SB4072 at 1:25 | `axb` | B68 |
| c05 | `str y='a)b'` / `echo "$(echo ${y:s/[)]/X/})"` | SB4057 at 2:6 | `aXb` | |
| r05 | `map m=([1]=20)` / `str k=']'` / `echo ${m[${k:s/]/1/}]}` | SB4047 at 3:6 | `20` | |
| r06 | the same map and key / `echo $m[${k:s/]/1/}]` | SB5064 at 3:6 | `20` | |
| r07 | r06 inside `"…"` | SB5064 at 3:6 | `20` | |
| r10 | `map m=([1]=20)` / `str q="'"` / `echo $m[${q:s/'/1/}]` | SB5064 at 3:6 | `20` | |
| c07 | `str y='4)'` / `int n=$(( ${y:s/[)]//} + 1 ))` / `echo $n` | SB4057 at 2:7 | `5`, see Caution | |
| c14 | `str q="'"` / `echo $(( ${q:s/'/1/} + 1 ))` | SB4057 at 2:6 | `2`, see Caution | |
| t2 | `str x='a}'` / `echo $(( ${x:s/a}/1/} + 2 ))` | SB4035 at 2:6 | `3`, see Caution | |

With `k=x` instead of `]` the map rows print `20` today, so only the extent
is wrong.

### Unclosed or malformed: should be SB4032 at the `${`

These compile today (B62):

| Id | Program | Today |
|---|---|---|
| n5 | `str x=abc` / `echo ${x:s/b/c/` / `echo after` | prints `acc`, then `after` |
| n6 | `str x=abc` / `echo "${x:s/b/c/` / `echo after"` | prints `acc`, then `echo after` |
| n7 | `str x=abc` / `echo "${x:s/b/c/"` | prints `acc` |
| p12 | `str x=abc` / `echo "${x:s/b/c/` / `}"` | prints `acc`, then `}` |

These are rejected today, under a code that points elsewhere:

| Id | Program | Today |
|---|---|---|
| k2 | `array a=(ab)` / `echo ${a[0]:s/b/c/` / `echo after` | SB4057 at 2:6, unterminated `${` |
| k5 | `array a=(ab)` / `echo "${a[0]:s/b/c/"` | SB4057 at 2:7 |
| m04 | `str x='abc'` / `echo "$(echo ${x:s/a)/b})"` | SB4057 at 2:6, unterminated `$(` |
| m05 | `str x='abc'` / `echo $(( ${x:s/a/1} ))` | SB4020 |
| m06 | `array a=(10 20)` / `str k=']'` / `echo ${a[${k:s/]/1}]}` | SB4047 |

Already SB4032, and must stay so: `${x:s/a/b}` (m01, and m07 inside `"…"`),
`${x:s/b/c/,` followed by a line that is not an edit (n4), `${x:s/a/b/g1}`
(m08, p5), `${x:s/b}`
(p8), `${x:s/b/c/z}` (p9), `${x:s/b/c/,}` (p11), `$a[${k:s/]/1}]` (m10, at
the inner `${`), and `$(( ${x:s/a/1/ junk} + 2 ))` (t5).

### Still rejected, under a different code once the extent is right

With `array a=(10 20)` and `str k=']'`, or `str q="'"` where shown:

| Id | Program | Today | After the fix |
|---|---|---|---|
| c08 | `echo $(( a[${k:s/]/1/}] ))` | SB4057 | SB4030, string cell in arithmetic |
| c23 | c08 inside `"…"` | SB4057 | SB4030, string cell |
| r08 | `map m=([1]=20)` / `echo $(( m[${k:s/]/1/}] ))` | SB4057 | SB4030, bare map in arithmetic |
| r11 | r08 inside `"…"` | SB4057 | SB4030, bare map |
| c09 | `echo ${a[${k:s/]/1/}]}` | SB4047 | SB4030, capture as an index |
| c10 | `echo $a[${k:s/]/1/}]` | SB5064 | SB4030, capture as an index |
| c11 | c10 inside `"…"` | SB5064 | SB4030, capture as an index |
| c15 | `echo $a[${q:s/'/1/}]` | SB5064 | SB4030, capture as an index |
| c12 | `echo $(( ${a[${k:s/]/1/}]} ))` | SB4057 | SB4041 |
| r09 | `map m=([1]=20)` / `echo $(( ${m[${k:s/]/1/}]} ))` | SB4057 | SB4041 |
| t6 | `str x='a)'` / `echo $(( ${x:s/a)/1/} + 2 ))` | SB4057 | SB5023, unmatched right paren |
| t7 | t6 inside `"…"` | SB4057 | SB5023 |
| t9 | `str x='a)'` / `echo "$(echo ${x:s/a)/1/})"` | SB4057 | SB5023 |

The expected codes come from the same programs with `k=x`, which have no
extent problem. They give SB4030 for c08/c23/r08/r11/c09–c11, and SB4041 for
c12/r09. c15 is c10 with a `'` in the payload; no `k=x` control was run for
it. Two other tasks change some of these expectations:
`bug-arith-braced-subscript-index-second-grammar` replaces the SB4041 of c12
and r09, and `bug-sed-index-capture-advice` rewrites the capture-as-index
message of c09–c11 and c15. Whichever task lands later re-pins these rows.
Already in place: `echo ${x:s/a)/1/}` (t8) is SB5023 at 2:6, and
`a[${k:s/]/1/}]=7` (c22) is SB4030 at 3:1.

### Already correct, and must stay so

- Statement form: `x =~ s/a"b/c/` gives `c` (k3), and `x =~ s/a/c/, s/}/d/`
  on `a}b` gives `cdb` (k4).
- Cell chain across a line break: `${a[0]:s/b/c/,` / `s/c/d/}` gives `ad`
  (k6). Cell with a `"`: `${a[0]:s/a"b/c/}` gives `c` (k7).
- A `}` in a scalar's first edit: `"${x:s/}/d/}"` gives `adb` (c3), and
  `${x:s/}/X/}` gives `aXb`, bare or quoted (c01, c02). Also
  `$(echo ${y:s/[)]/X/})` gives `aXb` (c06),
  `[[ ${x:s/}/]]/} == "a]]b" ]]` gives `yes` (c18), and
  `"[${x:s/}/X/,s/a/Y/}]"` gives `[YXb]` (c20).
- Positional and blank handling: `${1:s/b/c/}` gives `acc` (p1),
  `${x:s/b/c/ }` gives `acc` (p4), `${x:s/b/c/ ,s/c/d/}` gives `adc` (p6),
  and `${x:s/b/c/,s/c/d/ , s/d/e/}` gives `aec` (p7).
- Arithmetic: `$(( ${x:s/a/1/} + 2 ))` gives `3` (t1, and t4 as an `int`
  initialiser), and `int n=5` / `$(( ${n:s/5/7/} + 1 ))` gives `8` (t3).
  See Caution.

### Other rejections that stay

- A sed inside a `:-` default is SB4070, by DD#65's follow-on of 2026-09-30
  ("`:-` is a literal too"): `${e:-${x:s/}/X/}}` (c03), `$e:-${x:s/}/X/}`
  (c04), `$e:-$(echo ${y:s/[)]/Z/})` (c17), and the malformed m03 and m09.
- `echo ${x` / `}` stays SB4047 (n3). `@{a:s/b/c/}` stays SB5033 (p3).
  `${a[@]:s/b/c/}` stays SB4038 (p10).

### Caution: two other tasks re-code the arithmetic rows

`bug-arith-operator-expansion-unconverted` makes a sed import into
arithmetic SB4030, so the expected `5`, `2` and `3` above (c07, c14, t2)
become SB4030, and so do t1, t3 and t4. `bug-sed-sugar-on-int-escapes-dd105`
makes t3 SB4030 by a second route, since `n` is an `int`. Whichever task
lands first, the later one re-codes these rows. The extent fix stands either
way: each of them must stop failing with an extent error.

## Cause

Five scanners decide where a payload ends, and only one is the chain grammar:

- **Statement form** (`take_sed_stmt_payload` → `scan_sed_chain`): correct.
  It handles three fields, flags, `,` chains, blanks and a line break after a
  `,`, and it refuses a payload that does not end cleanly.
- **Scalar expansion** (`try_scan_sed_braced_content` → `scan_sed_payload`).
  It counts two `/`, and the first `}` after them closes. It stops at a `"`
  or a newline and knows nothing of `,`, which gives B63, B64 and B65. On an
  unterminated payload it returns its best guess and stops at the boundary,
  and nothing rejects that (B62), although the doc comment says `try_sed_op`
  does.
- **Cell and positional expansions.** `sed_prefix_end` needs an identifier
  starting with a letter or `_`, immediately followed by `:s/`. So
  `${a[0]:s/…}` and `${1:s/…}` fall through to `scan_by_brace_depth`, where
  the first unnested `}` closes (B67, B68).
- **Arithmetic and subscripts.** `scan_arith_braced_var` and
  `balanced_subscript_close` share `balanced_close`, which counts `[]`, `{}`
  and `()` and skips quoted spans. So a `)`, `]`, `'` or `"` inside a payload
  unbalances it. The bare arithmetic subscript `a[…]` uses a second,
  bracket-only scanner, `scan_balanced_brackets` in `parse/arith.rs`. The doc
  comments on `balanced_subscript_close` and `balanced_close` both say a
  second scanner must not exist.
- **A `$( )` inside `"…"`** (`step_dq_cmd_sub`) counts parentheses and skips
  quotes, so a `)` in a nested payload closes the capture (c05).

## Fix

Drafted during F15; DG has not reviewed it.

- One lexer function decides a payload's extent from just past `:s/`, with
  `scan_sed_chain`'s grammar, and returns not-a-sed, closed (end index) or
  malformed. The prefix test accepts what `scan_param_name` accepts as a
  name (digits included), optionally followed by a subscript balanced by
  `balanced_subscript_close`.
- Every scanner above calls it when it meets `${NAME:s/` or
  `${NAME[…]:s/`. The stepping and lookahead scanners skip a closed payload
  whole. `scan_balanced_brackets` is replaced by `balanced_subscript_close`.
  `@{a:s/…}` stays SB5033.
- A malformed payload records a scanner fatal, SB4032 at the `${`, which
  `scanner_fatal` ranks first so it wins over the follow-on SB4057, SB4072
  and SB4047. Check the order against F13's `with_scanner_fatal` at the
  re-lex sites.
- The `s_malformed` sentinel then has no producer:
  - `build_sed_sub` keeps its `Result` but drops the `Option`. Once the
    lexer has accepted a payload, a missing delimiter is a compiler bug, so
    it becomes an ICE, pinned by a `#[should_panic]` unit. First confirm
    that `split_sed_edit` rejects nothing `scan_sed_chain` accepts;
  - delete `build_sed_or_malformed`'s sentinel, and validate's
    `check_malformed_sed` / `flag_malformed_sed`;
  - `parse_regex_mutation` has two `sed_malformed_err` sites. Keep the one
    for `take_sed_stmt_payload`'s `None`; the one after `build_sed_sub`
    goes with the `Option`. Keep the unsupported-flag SB4032 (B66);
  - `parse_trusted_operator`'s `trust_malformed` sentinel cites
    `s_malformed` as its model, so update that comment.
- Specs: every row above, in both backends, with negative controls on the
  current binary.

## Open question: `$'…'` inside a payload (gates only that part)

DD#63's Lexer note says the brace scanner and the Path-B `/` scanner "must
skip a nested `$'…'` by **C-string (escape-aware) rules** so an escaped
delimiter inside (`$'\}'`, `$'\/'`, `$'\''`) does not terminate the field".
What happens today:

| Program | Result |
|---|---|
| `str x='a}b'` / `echo ${x:s/$'\x7d'/X/}` (also inside `"…"`, the statement form, and a cell) | `aXb` |
| `str x='a}b'` / `echo ${x:s/$'}'/X/}` | `aXb` |
| `str x='a"b'` / `echo ${x:s/$'\x22'/X/}` | `aXb` |
| `str x='a"b'` / `echo ${x:s/$'"'/X/}` | SB4057 at 2:14: B65, above |
| `str x='a"b'` / `x =~ s/$'"'/X/` | `aXb` |
| `str x="a'b"` / `echo ${x:s/$'\''/X/}` (every form) | `aXb` |
| `str x='a/b'` / `echo ${x:s/$'\x2f'/X/}` | `aXb` |
| `echo $'\/'` | prints `\/`: `\/` is not a DD#115 escape |
| `str x='a/b'` / `echo ${x:s/$'/'/X/}` (also a cell) | SB4032 at 2:6, "unsupported substitution flag `X`" |
| `str x='a/b'` / `echo ${x:s/$'\/'/X/}` | the same |
| `str x='a/b'` / `echo ${x:s/\//X/}` | the same: a plain `\/` does not escape the delimiter either |
| `str x='a/b'` / `x =~ s/$'/'/X/`, and the same with `$'\/'` | SB4032 at 2:1, "malformed `=~ s/…` substitution" |

Every byte but `/` can already be written, because Path B makes `}`
literal and the fields are decoded after the split. A raw `"` fails only in
the expansion form, through B65, and the extent fix above repairs it. A `/`
inside `$'…'` is a delimiter in every form. DD#63:139 and DG's later ruling in
`sed-sugar-ansi-c-quoted-replacement` spell a literal `/` as `\x2f`.

- **(a)** Supersede the Lexer note with an addendum: the payload is split on
  `/` before any `$'…'` is decoded, a literal `/` is `\x2f`, and the
  results above are the rule.
- **(b)** Implement the note: every extent scanner skips `$'…'` whole, by
  DD#115's escape rules, so `$'/'` is a literal `/`. That is a second
  quoting rule inside a field, for one byte.

Recommendation (made during F15, not yet ruled): (a).

Found during the batch-531 port (step F15, findings B62–B65, B67 and B68).
Filed under DG's rule for the port that questions and uncovered cases become
backlog tasks.
