title: Backlogging a Bash-compatible sash-expand
date: 2026-11-19

# Backlogging a Bash-compatible sash-expand

DG questioned three choices documented by `sash-expand(1)`: no `--help`,
no stdin default, and variable interpolation inside single quotes. He
clarified the intended purpose as a close approximation of Bash sourcing an
assignment file followed by env-like output of the variables within it.
The concrete use case is `/etc/os-release`.

DG then suggested using the tree-sitter Bash parser. The AI recommended
stock `tree-sitter-bash` with a strict non-executing evaluator rather than
another handwritten parser. DG requested: "capture this in the backlog".
This records the discussion for future implementation; it does not change
the helper or start that implementation.

## What the inspection established

At `be78a6f1`, `sash-expand/src/main.rs` requires exactly one filename,
splits the file into physical lines, strips either outer quote pair, then
calls the same expander on both. It updates its variable table and emits
each assignment immediately. The manual accurately describes that code,
but this is not Bash assignment-word parsing.

The current source and existing CLI tests were read; no new runtime
measurement accompanied this discussion. The UTF-8 corruption/stripping
panic and lone-quote panic remain the previously recorded findings from
the companion-manual work, not newly reproduced fixes.

`sash-convert` already depends on stock `tree-sitter`/`tree-sitter-bash` 0.25.
Its setup supplies an existing integration example, not an evaluator to
reuse. Its deliberate recovery/annotation behavior is unsuitable for a
loader which must refuse unsupported input. Tree-sitter can insert missing
tokens as well as produce `ERROR` nodes; both must be rejected.

The authoritative os-release format is smaller than the general expander
goal: shell-compatible assignments, UTF-8 text, no variable expansion, and
later entries winning when a file contains duplicate keys. That does not
require dropping safe expansion support for other configuration files.

## Backlog and boundaries

The new P2/open work owner is
[sash-expand-bash-assignment-loader](../tasks/sash-expand-bash-assignment-loader.md).
It records conventional file/stdin/help handling, Bash quote/escape and
supported expansion semantics, sequential evaluation, final file-owned
bindings rather than intermediate assignments, and strict refusal without
executing configuration. The larger helper-binary cost is explicit.

The two existing input-bug records remain open and now point to this
delivery scope. Their earlier instructions to preserve interpolation
inside single quotes and the nonrecursive operand shortcut are reconciled
in place; historical inputs, outcomes and source references remain.
The shared stdout-error task retains its cross-CLI ownership under DD133.
No new dependency between independently fixable tasks is invented.

The native compiler keeps its own parser and language rules. There is no
new `source` builtin, Bash subprocess, grammar fork, complete-shell
compatibility promise or implementation of additional Bash forms.
The shipped manual and DD54 remain unchanged until delivery. Status points
to the queued work; no bug is marked fixed.

This is documentation-only recording. No behavior probe, spec sweep,
coverage run or dependency change is performed; the last recorded gate
remains journal 661's acceptance sweep. Normal documentation review and
commit/push hooks remain the publication checks.
