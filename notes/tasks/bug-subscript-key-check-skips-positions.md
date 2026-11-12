title: "BUG: DD#80's key-type check (SB4030) runs on a private walker that misses most positions — a native ICE and a silent C miscompile"
status: open
priority: P1
kind: bug
depends: []
gates:
  - "type_check's key-type check is driven by the shared `ir` expression walker that SB4041 already uses; `check_subscript_types` and `check_subscripts_in_parts` are deleted"
  - "every row of the first two What tables is SB4030 in both backends, with the message its operator-free twin draws today"
  - "every row listed as already correct or masked keeps its verdict, and no diagnostic is reported twice"
  - "no subscript index reaches codegen's non-integer-literal ICE"
  - "reversion proof: restoring the private walker turns the new specs red"

## What

DD#80 types a subscript index by its collection: an indexed array is indexed
by number, a map by string. type_check enforces that as SB4030 in
`check_subscript_key_kind`, but only where its own walker reaches, and most
of the places an index can appear are out of its reach.

Results were measured after F15. Every row behaves the same on the base
commit 1309385 and on the batch-531 branch, so the bug is pre-existing and
the branch did not fix it. All rows declare `array a=(x y z)` unless they
show otherwise. Row pa1 was added 2026-10-31 by F10 (journal 552) and
measured on 3b96bc40 and the F10 tree, with the same result on both; it
was not measured on the base or the branch.

### A word as an index: the native backend ICEs, C uses index 0

Natively, each row panics with "non-integer literal `foo` reached int
lowering; type_check (SB4030) should have rejected it". The C backend
evaluates `foo` as 0 and runs.

| Id | Position | Program | C prints |
|---|---|---|---|
| ie2 | operator form | `echo ${a[foo]:-d}` | `x` |
| ie5 | operator form, assigned | `str v=${a[foo]:-d}` / `echo "$v"` | `x` |
| ie4 | operator form in `$(( ))` | `array a=(10 20)` / `echo $(( ${a[foo]:-0} + 1 ))` | `11` |
| ih1 | inside `$(( ))` | `int array a=(10 20)` / `echo $(( ${a[foo]} + 1 ))` | `11` |
| ih3 | operator form, `int` declaration | `int array a=(10 20)` / `int x=${a[foo]:-0}` / `echo $x` | `10` |
| p1 | `[[ ]]` operand | `[[ ${a[foo]} == x ]] && echo t` | `t` |
| p2 | `[[ -n ]]` operand | `if [[ -n ${a[foo]} ]] { echo t }` | `t` |
| p28 | `[[ ]]` right-hand side | `str y=x; [[ $y == ${a[foo]} ]] && echo eq` | `eq` |
| p31 | dispatch subject | `[[ ${a[foo]} == x { echo isx } ]]` | `isx` |
| pa1 | attest predicate operand | `v=$(arg 1)` / `attest v [[ $v == ${a[foo]} ]]` / `echo "ok $v"` | `ok x` given `x`; `y` fails the gate |
| p5 | array-literal element | `array b=(${a[foo]}); echo $(join b)` | `x` |
| p6 | map-literal value | `map m=([k]=${a[foo]}); echo ${m[k]}` | `x` |
| p21b | `$(<…)` path | `str v=$(<${a[foo]}); echo "${v:!}"` | tries to read a file named `x` |

Expected: SB4030, "non-integer index on indexed array 'a'", which
`echo ${a[foo]}` (ie6) draws today.

### A string index on an indexed array, or an int key on a map, compiles

Both backends print the same.

| Id | Position | Program | Prints |
|---|---|---|---|
| ic9 | operator form in `$(( ))` | `array a=(10 20)` / `echo $(( ${a[$(echo 1)]:-0} + 1 ))` | `21` |
| ic11 | the same, with a sed-expansion index | `array a=(10 20)` / `str k=x` / `echo $(( ${a[${k:s/x/1/}]:-0} + 1 ))` | `21` |
| ic12 | `:?` in `$(( ))` | `array a=(10 20)` / `int n=$(( ${a[$(echo 1)]:?bad} + 1 ))` / `echo $n` | `21` |
| ic13 | `(( ))` test | `array a=(10 20)` / `(( ${a[$(echo 1)]:-0} > 5 )) && echo big` | `big` |
| id1 | operator form | `echo ${a[$(echo 1)]:-dflt}` | `y` |
| id2 | operator form, assigned | `b=${a[$(echo 1)]:-dflt}` / `echo "$b"` | `y` |
| id3 | `:?` | `echo ${a[$(echo 1)]:?bad}` | `y` |
| id4 | substring | `echo ${a[$(echo 1)]:0:1}` | `y` |
| id6 | operator form inside `"…"` | `echo "${a[$(echo 1)]:-d}"` | `y` |
| p29 | `[[ ]]` operand | `[[ ${a[$(echo 1)]} == y ]] && echo eq` | `eq` |
| ie1 | string variable | `str s=1` / `echo ${a[$s]:-d}` | `y` |
| ih2 | string variable in `$(( ))` | `int array a=(10 20)` / `str s=1` / `echo $(( ${a[$s]} + 1 ))` | `21` |
| p30 | string variable, array literal | `str s=1` / `array b=(${a[$s]}); echo $(join b)` | `y` |
| ie3 | int key on a map | `map m=([1]=v)` / `int i=1` / `echo ${m[$i]:-d}` | `v` |

Expected: SB4030 with the message the same index draws in a position that is
checked: the capture message for a `$( … )` index (as id7 below), the
string-index message for `$s` (ie1p), and the map-key message for `$i`
(ie7).

### Already correct

| Id | Program | Today |
|---|---|---|
| ie6 | `echo ${a[foo]}` | SB4030, non-integer index |
| p9 | `echo "${a[foo]}"` | the same |
| p26 | `echo ${a[foo]}x` | the same |
| p25 | `str v=${a[foo]}; echo "$v"` | the same |
| p24 | `fn f { echo $1 }; f ${a[foo]}` | the same |
| p23 | `/bin/cat < ${a[foo]}` | the same |
| p10 | `echo $(echo ${a[foo]})` | the same |
| id7 | `echo ${a[$(echo 1)]}` | SB4030, capture as an index |
| id5 | `echo ${a[$(echo 1)]:s/y/Y/}` | the same |
| ie1p | `str s=1` / `echo ${a[$s]}` | SB4030, string-typed index |
| ie7 | `map m=([1]=v)` / `int i=1` / `echo ${m[$i]}` | SB4030, int key on a map |
| id8 | `echo "${a[$(echo 1)]:~}"` | SB5046 |

Each is reported once. That includes p10, whose index sits inside a capture,
so the gate against double reporting has a case to hold it to.

### Masked by an earlier rule

These positions could not be measured, because another rule fires first:

- p3, `str v=$(<${a[foo]})`, is SB5050, because `v` is never read. p21b
  above is the same position with `v` read.
- `for x in ${a[foo]}` (p4, and p22 with a second word) is SB5009.
- `${y:-${a[foo]}}` (p7, p8) is SB4070.
- `${a[foo]:+set}` (p12) is SB4039.
- `$(len ${a[foo]:-dd})` (ih4) is SB5062.

They keep their verdicts.

## Cause

`check_subscript_types` in type_check.rs is a private walker. Its arms cover
only `ExprKind::Subscript` and the parts of a `DoubleQuoted` or `Concat`, and
it ends in `_ => {}`. So it never looks at a parameter expansion's index
(with any operator), into `$(( ))`, into an array or map literal, or at a
`$(<…)` path. Its callers (check_assignment, check_declaration,
check_command and check_for) pass it values, arguments and `for` words, but
never a `[[ ]]` operand or a dispatch subject.

The other half of DD#80 does not have this gap. validate's SB4041
(`check_subscript_index`) runs on the shared total walker
`ir::walk_all_exprs`. With an ambiguous index `[i+1]` in place of `[foo]`,
SB4041 fires at every position above:

| Position | Program | SB4041 at |
|---|---|---|
| `[[ ]]` operand | `[[ ${a[i+1]} == y ]] && echo t` | 2:4 |
| dispatch subject | `[[ ${a[i+1]} == y { echo t } ]]` | 2:4 |
| `[[ -n ]]` operand | `if [[ -n ${a[i+1]} ]] { echo t }` | 2:10 |
| array literal | `array b=(${a[i+1]}); echo $(join b)` | 2:10 |
| map literal | `map m=([k]=${a[i+1]}); echo ${m[k]}` | 2:12 |
| `$(<…)` path | `str v=$(<${a[i+1]}); echo "${v:!}"` | 2:10 |
| operator form | `echo ${a[i+1]:-d}` | 2:6 |
| operator form in `$(( ))` | `int array a=(1 2)` / `echo $(( ${a[i+1]:-0} + 1 ))` | 2:6 |

The compiler already has a walker that reaches every one of these positions;
the key-type check does not use it.

DD#80's 2026-10-04 amendment counts three earlier gaps where its enforcement
"proved position-dependent rather than rule-dependent". It says the question
to ask is "what are all the IR shapes an index can inhabit". This is the
fourth such gap. The 2026-10-03 addendum records that walking assignment and
declaration values "incidentally closes an ICE — `str v="${a[foo]}"` reached
int lowering". The operator form of the same program (ie5) still reaches it.

This is also another instance for `sweep-private-duplicates-of-shared-walkers`,
the second in type_check.rs. `audit-validate-wildcard-escapes` (journal 422)
listed `check_subscript_types` as a suspect, for skipping `SlurpFile` and
`Arithmetic`. Its statement sweep found the key check firing in all eleven
statement containers, which still holds. Its expression sweep used seven
violating expressions, none of them a key-type mismatch, so the positions
above were never probed.

## Fix

Drive `check_subscript_key_kind` from the shared walker. Call it once per
statement through `ir::walk_own_exprs`, for every `ExprKind::Subscript` and
every parameter expansion that has an index. Use `walk_own_exprs`, not
`walk_all_exprs`: `check_stmt` already visits nested statements, so the
descending form would report each diagnostic once per enclosing level
(journal 308). `walk_expr_tree` also leaves `$( … )` bodies to that
recursion, which preserves the deliberate non-descent that
`check_subscript_types` documents. Then delete `check_subscript_types` and
`check_subscripts_in_parts`.

Specs: one per row of the first two tables, in both modes. SB4030 is raised
at many sites, so each spec asserts the distinguishing message text, not just
the code. The C rows are the negative control, since each prints a value
today.

## Related

- `bug-arith-braced-subscript-index-second-grammar`: inside `$(( ))`, an
  operator-free `${a[$(echo 1)]}` is SB4041 today, because of a second index
  grammar. Fixing that grammar moves its key check onto this one, so that
  task depends on this.
- `bug-sed-index-capture-advice`: ic11's index is a sed expansion. After this
  fix it draws the capture message, which that task rewords.
- B37 in `batch-531-findings`: command resolution keeps its own walk, which
  skips a parameter expansion's index. It is the same shape in another pass.
- B5 in `batch-531-findings` closed the END traversal gap (journal 565):
  `typeflow.10` pins the key check for `END { echo ${a[foo]} }`. The
  position-specific omissions in this task remain separate; their eventual
  repair will now reach both roots.

Found during the batch-531 port (step F15), while probing the sed-index
advice. Filed under DG's rule for the port that questions and uncovered cases
become backlog tasks.
