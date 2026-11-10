title: "Flag Bash option-flags expansions as unsupported during conversion"
status: open
priority: P3
kind: work
depends: []
gates:
  - "Actual Bash $- and ${-} option-flags expansions receive normal # MANUAL annotation and pass-through, not a new hard rejection or fabricated replacement"
  - "The braced option parameter is not rewritten as an empty-name default; literal occurrences receive no option-flags annotation"
  - "Existing compiler literal-word behavior and arithmetic/braced refusals remain unchanged in both backends"
  - "Focused converter before/reversion controls pin the specific annotation and braced preservation, independently of other hints or diagnostics"

## Decision, 2026-11-10

DG chose to keep the compiler's literal `$-` and place handling of Bash's
option-flags expansion in `sash-convert`, then clarified:

> reject is too strong a word - rather treat it as one of the unsupported bashisms

DD#64 retains word-level literal-dollar fallback. DD#57 assigns actual
Bash option-flags expansions to its existing manual category:
`# MANUAL:` annotation, unsupported construct passed through unchanged.
No new converter refusal mode, compiler rejection, Bash-flags emulation
or silent replacement with a supposed equivalent value.

The same Bash parameter written `${-}` follows the converter rule too.
That does not legalize `${-}` in native sash: its existing diagnostic
remains. Single-quoted, C-quoted and Bash-escaped literal occurrences are
not option-flags expansions and must not receive this annotation.

The question is settled, not implemented. This task retains its slug,
open status and P3 priority, becoming work for the converter. Journal 608
records the ruling and the current converter-only probe.

## Compiler behavior to preserve

The following tables are the historical F14 measurements, not new runs at
the decision date. Current source inspection confirms the literal fallback
in both word scanners. Arithmetic's SB4035 rule is separate (DD#72).

| Program | Historical result |
|---|---|
| `echo "[$-]" [$-]` | rc 0, prints `[$-] [$-]` |
| `echo "[%$%]"` | rc 0, prints `[%$%]` |
| `if [[ $- == *i* ]] { echo i }` | SB5086 at 1:7, "this comparison is decided at compile time — both operands of `==` are fixed by the source text; compare a variable instead" |

The constant-comparison diagnostic remains correct for native source:
there is no variable expansion in `$-`. Bash's different meaning is a
conversion concern, not evidence that the compiler's literal is wrong.

### Existing refusals

These programs keep their existing diagnostics; the decision does not
change arithmetic, braced-name parsing or the other parameter rules:

| Program | Result |
|---|---|
| `echo "[${-}]"` | SB4034 at 1:6, "the `-` parameter operator (in ${…}) isn't part of sash — …" |
| `echo "[$(( $- + 1 ))]"` | SB4035 at 1:6, "stray `$` in arithmetic — …" |
| `echo "[$0]"` | SB5017 |
| `echo "[$*]"` | SB5045, "sash has no `$*` — …" |
| `echo "[$#]"` (top level) | SB5024, "$# is function-local — use $(argc) at top level …" |
| `echo "[$_]"` | SB5031, "'$_' is assigned nowhere …" |

In an `s///` field DD#63 rules separately: a `$` that no name character
follows is a literal regex anchor. There `$-` is an anchor followed by `-`,
which never matches: `x=a-b; echo "[${x:s/$-/X/}]"` prints `[a-b]`, and the
control `${x:s/-b$/X/}` prints `[aX]`.

## Source boundary

Both word scanners in `sash/src/parse/lexer.rs` recognise a special
parameter from a fixed set, `@ * # ? ! $` and the digits, which has no `-`:
`Scanner::scan_dollar` (unquoted) and `Scanner::scan_dq_dollar`
(double-quoted). After any other byte, the `$` is kept as literal text and
the byte is left for the next token, so `$-` reaches the program as the two
characters `$-`. Keep that behavior; do not add `-` to those special
parameter sets for this task.

`sash-convert/src/convert.rs` already annotates `set` and `shopt` as
unsupported shell-option operations. Reuse the normal manual category
for option-parameter expansions, using Bash syntax rather than searching
raw text for `$-`.

## Current converter evidence, 2026-11-10

A probe using the existing `target/release/sash-convert` at the clean
policy-recording baseline `95bff850` returned success with empty stderr:

| Bash input | Converted output |
|---|---|
| `echo "[$-]" [$-]` | unchanged, no annotation |
| `echo "[${-}]"` | `echo "[${:-}]"`, no annotation |
| `echo '$-' '${-}'` | unchanged, no annotation |
| `echo $'$-'` | unchanged, no annotation |
| `echo "\$-" "\${-}"` | unchanged, no annotation |

The braced case is a coupled converter defect, not just a missing hint.
`convert_expansion`'s `${NAME-default}` rewrite accepts an empty
`before_dash` because `chars().all(...)` is true on an empty string.
Consequently `${-}` becomes `${:-}`. The queued work must preserve the
unsupported parameter rather than allowing that mechanical rewrite to
consume it.

The probe did not compile or execute its output. The literal controls
establish only conversion output and absence of an option-flags annotation,
not correctness of every unrelated Bash-to-sash conversion.
Source/stdout/stderr are retained in the session's
`files/dollar-flags-20261110.{bash,out,err}`.

## Implementation and proof

- Cover actual option-flags expansions in unquoted and double-quoted
  words, braced spelling, condition operands and function bodies. Their
  placement does not turn the Bash parameter into native literal text.
- Preserve the unsupported expansion using DD#57's existing annotation
  machinery. Do not silently quote it into a literal or invent flag values.
  Other mechanical conversions and unsupported-bashism annotations remain.
- Literal controls must not receive the option-flags annotation; they may
  still need unrelated conversions or annotations. A comment mentioning
  `$-` is not an expansion either.
- Add focused coverage in `spec/convert.sh`, checking the distinguishing
  annotation and preservation of `${-}`, not just any `MANUAL` line.
  The existing `case` annotation must not masquerade as proof of this rule.
  Take fresh ids above the database high-water mark and measure the actual
  impact before predicting a gate.
- Prove the new annotation and braced preservation against the before
  converter and by reversion. Keep compiler literal/refusal controls in
  both backends, reusing existing coverage where it already pins the claim.
  An annotated unsupported Bash program is not required to compile or
  reproduce Bash's flags before human review.

Found during the batch-531 port (F14, B38; journal 533). The earlier
compiler-rejection recommendation is superseded by DG's converter-boundary
decision, not implemented as an additional compiler restriction.
