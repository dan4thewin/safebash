title: "Bug: sash-convert reads `$10`, `$01` and `$1x` as one positional name at top level"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "at top level, an unbraced `$` positional hoists one digit only, in a word and in a double-quoted string: `$10` reads argument 1 then `0`, `$01` reads `$0` then `1`, `$1x` reads argument 1 then `x`"
  - "the converted programs below print what bash prints, in both backends"
  - "braced `${10}` and `${01}`, top-level `$1` (57.13, conv.x3a) and the in-function pass-through (conv.x8) keep their current conversion"
  - "a convert.sh spec pins each form; it fails on the pre-fix converter"

## What

Measured with arguments `a` … `j`, in both backends:

| Top-level form | bash | sash-convert emits | Converted program |
|---|---|---|---|
| `$10` | argument 1, then `0`: `a0` | `$arg10`, from `arg10=$(arg 10)` | argument 10: `j` |
| `"$10"` | `a0` | `"$arg10"` | `j` |
| `$01` | `$0`, then `1` | `$arg01`, from `arg01=$(arg 01)` | argument 1: `a` |
| `$1x` | argument 1, then `x`: `ax` | `$1x`, unchanged | SB5024, "$1 is function-local — use $(arg 1) at top level" |
| `${10}` | `j` | `$arg10` | `j` (correct) |
| `${01}` | `a` | `$arg01` | `a` (correct) |

For `echo ${10} $10 ${01}`, bash prints `j a0 a`, and the converted program
prints `j j a`. The hoist line keeps its MANUAL note, which names the form
as if it were one argument: "sash-convert cannot know what $10 must look
like".

Inside a function body the converter passes positionals through unchanged
(DD#46/DD#60, conv.x8). sash's own lexer reads one digit after an unbraced
`$` (`Scanner::scan_dollar`), so `$10` there is argument 1 then `0`, as in
bash. A braced `${10}` inside a function is a separate bug:
`bug-multi-digit-positionals`.

No convert.sh spec covers a digit-led name longer than one character; 57.13
and conv.x3a cover `$1`.

## Cause

tree-sitter-bash 0.25.1 names a `simple_expansion` with the whole run of
word characters after the `$`: `_simple_variable_name: alias(/\w+/,
$.variable_name)` in its `grammar.js`. So the names are `10`, `01` and
`1x`; bash takes one digit.

`convert_simple_expansion` (`sash-convert/src/convert.rs`) passes an
all-digit name to `positional_hoist` when it is outside a function. `10`
and `01` are hoisted whole. `1x` is not all digits and is not a known
environment variable, so no patch applies, and `$1x` reaches sash
unchanged.

## Fix (proposed, not yet ruled)

At top level, when the name starts with a digit, hoist the first digit only,
and emit the rest as literal text after a braced reference: `${arg1}0`,
`${arg0}1`, `${arg1}x`. The braces are required, because sash reads `$arg10`
as a variable called `arg10`. Inside a function nothing changes.

Standalone `convert.sh` runs need `target/release` prepended to `PATH`, or
they test the installed sash-convert (AGENTS.md, Spec tests).

Found during the batch-531 port (step F14, finding B38). Filed under DG's
rule for the port that questions and uncovered cases become backlog tasks.
