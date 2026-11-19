title: Make sash-expand a Bash-compatible assignment-file loader
status: open
priority: P2
kind: work
depends: []
gates:
  - "The real CLI supports --help, file input, and stdin when no filename or - is supplied"
  - "Supported assignment words follow Bash quoting, escaping, comments and continuation rules; UTF-8 is preserved"
  - "Stock tree-sitter-bash supplies syntax; a strict non-executing evaluator rejects unsupported constructs and all ERROR/missing-token recovery"
  - "Expansions use inherited values and preceding assignments; output contains each file-assigned name once with its final value"
  - "The existing UTF-8 and lone-quote defects are repaired with real CLI evidence; stdout handling follows DD133"
  - "Focused compatibility/refusal specs, original-binary controls and independent reversions pass through the ordinary predicted gate; the manual and DD54 accurately describe delivery"

## Goal and scope

DG described the goal on 2026-11-19 as:

> a close approximation of bash sourcing a file followed by env-like output of the variables within

The concrete use case is `/etc/os-release`. DG questioned the current
file-only CLI, lack of `--help`, and interpolation inside single quotes,
then suggested the tree-sitter Bash parser and requested this backlog record.
This is queued implementation work, not a claim that the helper has changed.

Keep [DD54's data-loading boundary](../decisions/DD54-source-pipeline.md):
configuration is not executed. This does not add `source`/`.` to native
sash, invoke Bash on user input, or round-trip the resulting variables
through the caller's environment.

The [os-release format](https://www.freedesktop.org/software/systemd/man/latest/os-release.html)
uses shell-compatible assignments but explicitly does not support variable
expansion. Correctly reading those files is the basic use case, not a reason
to remove the expander's broader safe parameter operations. Readers should
use the later entry if a file repeats a key, although authors should not
repeat keys.

## Intended behavior

- Accept `sash-expand [FILE]`: an omitted filename or `-` reads stdin;
  `--help` describes the command rather than opening a file of that name.
- Parse assignment words rather than trimming physical lines and removing
  one outer quote pair. Single quotes preserve literal contents; double
  quotes allow expansion with Bash's context-dependent backslash rules.
  Handle mixed quoted/unquoted word pieces, comments at shell comment
  positions, and line continuations without corrupting data.
- Retain the safe expansion family: simple references, defaults, alternates
  and prefix/suffix stripping. Supported forms must follow Bash semantics,
  including expansion of selected default/alternate operands, rather than
  silently treating their contents as literal text. Assignment values do
  not undergo command-argument word splitting or pathname expansion.
- Seed evaluation from inherited variables and apply assignments in source
  order. Emit raw `KEY=value` records for file-assigned names only, once per
  name with the final value. A later assignment wins; ordinary assignments
  need no export marker to appear. Do not dump unrelated inherited variables
  or emit an intermediate-assignment event stream.
- Reject actual commands, command substitutions and other unsupported
  syntax, including when nested in a value. Quoted text which merely
  resembles executable syntax remains data. Unsupported input must not
  become a successful empty value, a skipped node or a Rust panic.

This is a restricted Bash assignment evaluator, not complete Bash sourcing
or an os-release-only parser. Additional forms such as export declarations,
arrays, arithmetic and indirect expansion are not selected by this record.
State the supported subset and pattern/character rules explicitly when
implementing it; do not infer full Bash compatibility from parser acceptance.
The native language's intentionally different word rules are not the
compatibility authority for this Bash-input helper.

## Parser and evaluator

Use stock `tree-sitter-bash` for syntax instead of another handwritten shell
parser. `sash-convert/Cargo.toml` already uses `tree-sitter` and
`tree-sitter-bash`; its parser setup is in `sash-convert/src/convert.rs`.
Reuse that dependency family, not the converter's translation rules or its
recover-and-preserve handling of input it cannot parse.

Tree-sitter supplies a concrete syntax tree, not cooked strings or evaluated
variables. The helper still needs explicit quote/escape processing,
parameter evaluation and a variable table. Accept a closed set of complete
forms, accounting for both named nodes and operator tokens; do not merely
extract assignment nodes from a larger executable program. Reject `ERROR`
nodes and inserted missing tokens before accepting the input.

The tradeoff is a larger helper binary in exchange for reusing Bash syntax
machinery already in the build. No tree-sitter dependency is reintroduced
into the native compiler, and the historical grammar fork is not revived.

## Existing owners and delivery

The [UTF-8 processing bug](bug-expand-utf8-value-processing.md) and
[lone-quote panic](bug-expand-lone-quote.md) belong in this rework. Their
historical reproducers remain useful, but preserving interpolation inside
single quotes is no longer a repair constraint. Keep both records open
until their actual regressions are proved fixed.

[DD133](../decisions/DD133-cli-stdout-failures.md) governs output errors,
including help and final-variable output. Coordinate the expander's portion
with [the shared stdout owner](bug-stdout-write-failure-panics.md); this
helper's repair does not close the outstanding sash/converter work.

Use the real checkout helper in the existing spec harness. Cover file/stdin
parity, quoting and escaping, comment/data distinctions, selected nested
expansions, Unicode literals and inherited values, reassignment, inherited-only
names, malformed syntax and forbidden nested constructs. Include literal
command-looking text as a preservation control. Compare the supported subset
against Bash using owned, inert fixtures, not by sourcing arbitrary system
configuration during tests.

Migrate existing assertions that encode the old non-Bash format rather than
weakening their checks. Take fresh historical ids for changed contracts,
predict the gate after the fixture inventory is known, and prove the
parser/evaluator refusal boundaries as well as successful output. Update
`docs/sash-expand.1` and the relevant DD54 description with the implemented
contract; do not advertise this backlog as shipped behavior.
