---
title: "brace-block control flow (if/while/until/for) + then/fi/do/done sugar (DD#68)"
status: ready
priority: P2
kind: feature
depends: [DD68-brace-control-flow, DD67-native-syntax-sugar-policy, DD66-cond-form-dispatch]
gates:
  - "TDD: spec tests below written first and red before code"
  - "if/while/until/for accept brace { } bodies; lower to the SAME existing IR"
  - "condition is a command list ending at the first top-level {; brace-group conditions rejected"
  - "then/fi & do/done still accepted (1-to-1 sugar, DD#67); no mixing within one construct"
  - "else if accepted as else + nested if; elif unchanged"
  - "$? / break / continue / redirects unchanged (parser-only)"
  - "native spec clean; rust suite no net regression; complexity gate green"
---

# brace-block control flow (DD#68)

Implements DD#68. `if`/`while`/`until`/`for` gain `{ }` bodies;
`then/fi`/`do/done` stay as DD#67 sugar. Parser-only — both forms build
the existing `If`/`While`/`For` IR, so semantics are unchanged.

Spec-first. The brace forms do not exist yet (all REJECT today, SB4014).

## Spec tests to write first

### A. `if` brace bodies

- **A1** then-arm: `if true { echo y }` → `y`.
- **A2** else: `if false { echo y } else { echo e }` → `e`.
- **A3** elif: `x=2; if [[ $x == 1 ]] { echo one } elif [[ $x == 2 ]] { echo two } else { echo z }` → `two`.
- **A4** `else if` sugar: `x=3; if [[ $x == 1 ]] { echo a } else if [[ $x == 3 ]] { echo c } else { echo z }` → `c`.
- **A5** command condition: `if grep -q x <<< xyz { echo found }` → `found` (or the available redirect idiom).
- **A6** compound condition: `if true && true { echo y }` → `y`; `if true && false { echo y } else { echo n }` → `n`.
- **A7** list condition (group-equivalent): `if false; true { echo y } else { echo n }` → `y` (status = last = true).

### B. `while` / `until` / `for`

- **B1** while: `i=0; while [[ $i -lt 2 ]] { echo $i; i=$((i+1)) }` → `0\n1`.
- **B2** until: `i=0; until [[ $i -ge 2 ]] { echo $i; i=$((i+1)) }` → `0\n1`.
- **B3** for-in: `for x in a b c { echo $x }` → `a\nb\nc`.
- **B4** C-style for: `for ((i=0;i<2;i++)) { echo $i }` → `0\n1`.
- **B5** break/continue unchanged: `for x in a b c { if [[ $x == b ]] { continue }; echo $x }` → `a\nc`.

### C. Sugar still works (regression, DD#67)

- **C1** then/fi: `if true; then echo y; fi` → `y`.
- **C2** do/done: `for x in a b; do echo $x; done` → `a\nb`.
- **C3** while do/done: `i=0; while [[ $i -lt 1 ]]; do echo $i; i=$((i+1)); done` → `0`.

### D. Disambiguation + rejections

- **D1** brace-group as condition rejected: `if { true; } { echo y }` → rejected
  (new SB code; "a `{ }` group cannot be a condition — use a `;`-list").
- **D2** no mixing then↔brace: `if true; then echo y }` → rejected;
  `if true { echo y; fi` → rejected.
- **D3** missing body: `if true` then EOF/next-stmt with no `{`/`then` → rejected
  ("expected `{` body or `then` after the condition").
- **D4** literal `{` in for-list via quoting: `for x in '{' a { echo $x }` → `{\na`.
- **D5** reserved `{` unaffected: `echo a{` still SB4014 (regression).

### E. Semantics unchanged (parser-only)

- **E1** `$?` catching: `if true { false }; echo $?` → `0`.
- **E2** else-arm `$?` catching: `if false { true } else { false }; echo $?` → `0`.
- **E3** body redirect: `if true { echo y } > /dev/null` → (no output, rc 0).
- **E4** nested: `if true { if true { echo y } }` → `y`.

## Implementation outline (after specs are red)

1. **Parser** (`parse_if`/`parse_while`/`parse_for`): after the condition
   (parsed as a command list terminated by the first top-level `{`, `then`,
   `do`, or newline-then-keyword), branch on the delimiter:
   - `{` → parse a brace block body (reuse the existing brace-group parse),
     then `}`; for `if`, accept `elif`/`else`/`else if` with brace bodies.
   - `then`/`do` → the existing sugar path (`fi`/`done`).
   Reject mixing. Reject a brace-group as the condition (the condition
   list must not begin with, or contain at top level, a `{`).
2. **No IR/transform/codegen change** — both forms produce the existing
   `If`/`While`/`For` IR. Verify the catching-construct `$?` path is hit
   identically (E1–E2).
3. **Diagnostics**: new SB code for "expected `{`/`then` body" (D3) and
   "brace-group condition" (D1). The "no mixing" cases (D2) reuse the
   relevant expected-token errors.
4. **`for` head**: the `in` LIST ends at the first top-level `{`; a literal
   `{` element is quoted (D4).
5. **Docs**: GUIDE.md / SUBSET.md show the brace forms as canonical, with
   `then/do` noted as accepted sugar.
6. **Backends/converter**: emit/janet/c already consume the shared IR (no
   change). `sash-convert`/`--emit-source` per DD#67 (default preserves;
   `--native`-style switch normalizes to braces — see `dd67-bash-sugar-sweep`).

## Cross-refs

- `dd67-bash-sugar-sweep` — the convert switch + `name()` re-add + the
  rejected-form sweep.
- DD#66 dispatch `[[ ]]` — the other `{ }`-block construct.
