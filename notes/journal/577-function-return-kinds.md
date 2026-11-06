title: Function return kinds without ceremony
status: complete

DG's journal 574 return-kind decisions and journal 575 unattended assignment
are implemented by Copilot in session
ed4c7981-c5a0-4e82-9832-9557dc269030 on `exit-fail-return-kinds`.
Unit A shipped separately as 0d0a8210; this is Unit B.

Bare return and fall-through supply no result. Explicit zero is a status
in an unannotated function and an integer in an `-> int` function.
Shared `lower_body` checks named and anonymous capture bodies independently
of caller handling. It reuses the canonical classifier and the definition
in force, including the existing static-template treatment of redefinitions.
SB4037 rejects missing results; SB4030 rejects incompatible forwarded kinds.
Operand presence is retained as an expression, not inferred from rendered
text. Redirects preserve the forwarded kind. Their operational open failure
cannot manufacture a void/integer result.

Self-tail edges and exit/fail are not missing-result exits. The existing
definite-return helpers now recognize terminating conditions and statements;
return collection excludes paths after them, including unreachable loop,
elif and short-circuit bodies. Small helpers keep these changes below the
live complexity gate rather than bypassing it. SB4017, its scan and its
sole-purpose zero helpers are removed without another constant-result rule.
Direct integer-function pipeline stages are SB4030 in all otherwise legal
positions, including suppressed stages. Integer captures, compatible
forwarding, in-process discard and explicit-status wrappers remain valid.

The undispositioned return operand guard applies DD89's already-settled
integer rules, including empty and quoted strings. Existing
`return ${s:-N}` / `${s:?message}` conversions remain unchanged. No choice
is made on their separate disposition policy, numeral/producer grammar,
anonymous-capture caller-status handling or redefinition policy.
`bug-return-status-string-operand` records the partial repair and stays open;
DD89's incorrect attribution of its operand guard to journal 411 is corrected.

`return_kinds.sh` adds 56 dual-mode checks, 112 checks net. Nine existing
checks migrate to fresh `migrateb.1`-`.9` IDs: void tail loops use bare
returns; an integer elif fixture supplies the unmatched path; missing or
incompatible results assert their replacement diagnostic; explicit integer
zero is accepted; the former quoted-number status fixture preserves its
output via explicit arithmetic conversion. The migration ledger is private.

The frozen Unit A compiler/runtime fail 35 of the final 56 checks per mode,
with 21 positive controls. Both native and C controls link the intended
archive; native uses the existing private ld wrapper. Independent reversions
turn red, per backend: completeness 8; producer-kind compatibility 5;
zero-as-void classification 8; redirected-kind preservation 6; direct
integer-stage refusal 6; redirect-open failure handling 2; operand typing 3.
Two further controls turn red 4 each for unreachable return-site collection
and terminating-condition completeness. Every stub is restored. Saved
source and ordinary compiler/runtime hashes match after rebuild and gate.

The first B sweep, rowids 6845431-6856759, retained its 20 failures in
history: five fixtures still encoded old contracts, and the provisional
blanket return-sink check wrongly rejected five established disposition
fixtures. Fixing the latter preserves their existing assertions and policy.
The predicted second gate passes at 6856760-6868096:
11324 passing, 13 skipped, no failures; NEW=18, RETIRED=10, FLIPS=10,
TOTAL=11337. The flips are those ten restored disposition mode IDs.
After the final terminal-condition edges and complexity extractions, the
ordinary `NEW=8 RETIRED=0 FLIPS=0 TOTAL=11345 mk -p 1 gate` passes at
6868097-6879441: 11332 passing, 13 skipped, no failures. Unit tests,
static census, history predictions and liveness all pass. Normal pre-commit
formatting, clippy, totality and complexity gates pass without bypasses.

Evidence is in the assigned private `exit-fail-return-kinds-unattended`
directory: `before-b-final-*`, `revert-b-*`, `restored-b-final-*`,
`gate-b1.txt`, `gate-b2.txt`, `gate-b3.txt`, and `migration-b.tsv`.
The completed tasks are `enforce-function-return-kinds`,
`retire-return-ceremony-check` and `q-int-function-pipeline-status`.
No coverage or DD129 work was undertaken.
