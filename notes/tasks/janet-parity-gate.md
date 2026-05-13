title: Janet parity quality gate
status: done
severity: infrastructure
gates: [15.1, 15.2]

Three-aspect gate ensuring native and Janet backends stay in sync:

## 1. Output parity (runtime)

Every codegen test runs the script through both backends:
- Compile to native binary, execute, capture stdout
- Emit Janet via emit_janet, execute with `janet`, capture stdout
- Diff. Any difference fails the test.

This is a harness change, not 268 new test functions. Each codegen
test gets one additional call. `mk test` fails on any mismatch.

## 2. Emission completeness (static)

No unhandled IR nodes in emit_janet.rs. Every match arm in codegen.rs
that handles an SsaInst variant must have a corresponding handler in
emit_janet.rs. Enforce via:
- Exhaustive match (already the case if both use `match` on SsaInst)
- CI check: grep for `todo!()` or `unimplemented!()` in emit_janet.rs

## 3. Feature-gated (falls out of #1 + #2)

Adding a new IR node without a Janet implementation fails #2 at
compile time (unhandled match arm) and #1 at runtime (missing output
or panic). No separate enforcement needed.

## Notes

When sash deliberately diverges from bash (DD#31 arithmetic, DD#38
subshells, etc.), the gate compares native-vs-Janet, not Janet-vs-bash.
Both backends implement sash semantics (DD#15).
