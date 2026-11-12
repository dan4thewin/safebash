title: "BUG: a substring or sed expansion imported into arithmetic converts to int with no disposition (DD#77)"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "in arithmetic, a string-typed expansion with any operator but `:-` and `:?` is SB4030, for scalars and cells alike, the sed sugar included"
  - "`:-` and `:?` still discharge the obligation (ao7, ac3, ao9), and every row listed as unchanged keeps its verdict"
  - "relex.19 keeps SB5011"
  - "the arithmetic rows of `bug-sed-expansion-extent` (see its Caution) are re-coded by whichever of the two tasks lands second"

## What

DD#77 (D1) says that in arithmetic a string operand "**Must** carry a
disposition; a bare string operand is a **compile error**". The dispositions
(D2) are `${s:-N}` and `${s:?msg}`.

A substring or a sed expansion of a string is a string. In arithmetic,
though, it converts to int with no disposition, by lenient atoi, so `a`
becomes 0. Results were measured after F15, and are the same in both
backends, on the base commit 1309385 and on the batch-531 branch. The one
exception is ar3, under "Unchanged" below: base, which predates F14, reports
it as SB4020.

| Id | Program | Prints |
|---|---|---|
| ao3 | `str y=abc` / `int n=$(( ${y:0:1} + 1 ))` / `echo $n` | `1` |
| ap6 | `str y=abc` / `int n=$(( ${y:1} + 1 ))` / `echo $n` | `1` |
| ao6 | `str y=abc` / `int n=$(( ${y:s/x//} + 1 ))` / `echo $n` | `1` |
| s2 | `str y=4` / `int n=$(( ${y:s/x//} + 1 ))` / `echo $n` | `5` |
| ac1 | `array s=(abc)` / `int n=$(( ${s[0]:0:1} + 1 ))` / `echo $n` | `1` |
| ac2 | `array s=(abc)` / `int n=$(( ${s[0]:s/x//} + 1 ))` / `echo $n` | `1` |
| ai2 | `int y=123` / `int n=$(( ${y:s/1/9/} + 1 ))` / `echo $n` | `924`; see also `bug-sed-sugar-on-int-escapes-dd105` |

s2 shows that the rule is about the operand's type, not its value: `4` is a
valid integer, but no disposition was written.

Expected: SB4030, as the operand draws without its operator. ao8, for
example, gets "bare string `y` in arithmetic — a string→int conversion must
carry a disposition: `${y:-N}` … or `${y:?msg}` …".

A disposition cannot be attached to the operator form (`${y:s/x//:-0}` is
SB4032, ar4), so the message has to point at a variable:
`str c=${y:0:1}` then `${c:-0}`.

### Unchanged

Each operand below appears as `int n=$(( OPERAND + 1 ))`, after `str y=abc`
(or `array s=(abc)` for the `s[0]` rows). Two rows differ: ar3 is
`echo $(( OPERAND + 1 ))`, and ai1 declares `int y=123`.

| Id | Operand | Today |
|---|---|---|
| ao8 | `${y}` | SB4030, bare string |
| ac4 | `${s[0]}` | SB4030, string cell |
| ao7 | `${y:-0}` | `1` |
| ac3 | `${s[0]:-0}` | `1` |
| ao9 | `${y:?bad}` | aborts with `bad` |
| ao1, ao2 | `${y#a}`, `${y%c}` | SB4034 |
| ap2, ap3, ap4, ap5 | `${y:=0}`, `${y:+1}`, `${y^^}`, `${y/a/b}` | SB4034 |
| ao4 | `${#y}` | SB4038 |
| ao5 | `${y:!}` | SB4030 |
| ap1 | `${y:~}` | SB5046 |
| ar3 | `$(sub $y x "")` | SB4035, a capture is not an arithmetic operand |
| ar4 | `${y:s/x//:-0}` | SB4032 |
| ai1 | `${y:0:1}` | SB4030, substring of an int variable |

## Cause

Three exemptions in type_check.rs let these operands through:

- `check_scalar_import_operand` returns early when
  `pe.operator.is_some() || pe.index.is_some()`. Any operator therefore
  exempts a scalar, not only the two dispositions.
- `undisposed_cell_name` matches a cell only when `pe.operator.is_none()`, so
  any operator exempts a cell.
- The parser desugars the sed sugar to a `CommandSubstitution`
  (`$(sub "$y" …)`) before type checking. `check_scalar_import_operand`
  returns early for anything that is not a parameter expansion. Written out
  by hand, the same capture is SB4035 in arithmetic (ar3).

The predecessor is `bug-arith-param-expansion-operator-zero` (done). It made
an operator-bearing `${…}` a real expansion inside arithmetic (DD#77 Stage
A), which is how these operator forms reach the checks above at all.

## Fix

- Exempt only `:-` and `:?`, in both functions.
- Treat a desugared sed in an arithmetic operand as an undisposed string.
- Word the message so that it names an intermediate variable, as
  `bug-sed-index-capture-advice` does for an index.

relex.19 (`int n=$(( ${x:s/5/$(nosuchcmd)/} + 1 ))`) must keep SB5011.
SB5011 is raised by command resolution and by validation, and the compiler
exits after either before type checking runs, so it should.

`bug-sed-expansion-extent`'s Caution lists the rows this task turns into
SB4030: t1, t3, t4, c07, c14 and t2.

Found during the batch-531 port (step F15), while probing the arithmetic
rows of the sed-extent family. Filed under DG's rule for the port that
questions and uncovered cases become backlog tasks.
