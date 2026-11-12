title: "BUG: sash-convert collapses whitespace inside a quoted test operand, and splits the operand at `&&` / `||`"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "every program under What converts to one that prints what bash prints (w8 prints `nonempty`)"
  - "a converted test keeps the exact bytes of every operand it does not rewrite: blank runs, tabs, newlines, and an `&&`, `||`, `=` or `!` inside quotes"
  - "specs in spec/convert.sh pin w5, w7 and w8; the controls w4 and w9 keep converting as they do today"

## What

When sash-convert rewrites a test (`[ ]` to `[[ ]]`, `=` to `==`, or a lone
operand to `-n`), it re-tokenises the test's text with `split_whitespace` and
joins it back with single spaces. The tokeniser is `fix_test_operators`
(sash-convert/src/convert.rs:1301), which hands each `&&`/`||` segment to
`fix_test_segment` (1323). Neither is quote-aware. So every whitespace run
inside a quoted operand becomes one space. An `&&` or `||` inside quotes is
taken as a connective, and the lone-operand rule then fires on a fragment.

| Id | Bash | Converted | Bash prints | Sash prints |
|---|---|---|---|---|
| w1 | `x="a  b"` / `[[ $x == "a  b" ]] && echo same` | `[[ $x == "a b" ]]` | `same` | nothing |
| w2 | `x="a  b"` / `if [ "$x" = "a  b" ]; then echo same; fi` | `if [[ $x == "a b" ]]; …` | `same` | nothing |
| w3 | a tab between `a` and `b` inside the quotes | `"a b"` | `tab` | nothing |
| w5 | `x="  a"` / `[[ "$x" == "  a" ]] && echo lead2` | `" a"` | `lead2` | nothing |
| w6 | a newline between `a` and `b` inside the quotes | `"a b"` | `nl` | nothing |
| w7 | `x="a && b"` / `[ "$x" = "a && b" ] && echo same` | `[[ $x == "a && -n b" ]]` | `same` | nothing |
| w8 | `[ "a b" ] && echo nonempty` | `[[ "a b" ]]` | `nonempty` | SB4068 |

w1 needed no rewrite at all. An already-`[[ ]]` test is patched whenever the
re-joined text differs from the input (`convert_test_command`'s second call
site, 1238), and the collapse alone makes it differ. w8 splits into two
tokens, so the lone-operand rule (bash's implicit `-n`, DD#118) does not see
a lone operand.

Controls: a single blank survives anywhere (w4 `" a"`, w9 `"a = b"`), because
the re-join uses one space. A non-test `echo "a  b"` is untouched. HEAD
before D1 (bf48bec) converts every program above identically; D1 did not
touch these functions.

## Fix

Recommendation (made during D1, not yet ruled): tokenise with a quote-aware
scanner (single and double quotes, `$( )`, `${ }`, backslash). Then splice
only the tokens a rule changes (`=` to `==`, an inserted `-n`) back into the
original text, so every untouched byte survives. `try_numeric_comparison`
(1358) splits with `split_whitespace` the same way and should share the
tokeniser.

## Related

- DD#118: the loose test spellings these functions rewrite.
- `bug-convert-output-fails-to-compile`: its x05 is w2 followed by `|| echo
  collapsed`.

Found during the batch-531 port (step D1, while checking the converter's
test rewrites for the new int quoting). Filed under DG's rule for the port
that questions and uncovered cases become backlog tasks.
