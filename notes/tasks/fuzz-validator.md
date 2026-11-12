title: Fuzz target for the front-end (differential vs bash)
status: ready
depends: []
gates:
  - tests
  - commit

No `cargo-fuzz` or property-test harness exists. Needed to find validator
bypasses the adversarial audit didn't catch — and, per the 2026-08-29
param-operator session, the *silent-divergence* class the audit's threat model
misses entirely.

## Reframed by the 2026-08-29 param-operator session (journals 209-211)

DG's `${x-4}` question surfaced bugs (`${x-4}` `${x+4}` `${x&y}` …) that are
**not** crashes and **not** taint bypasses. `${x+4}` compiled cleanly and
returned `$x` (5) instead of the alternate (bash: 4) — a silently *wrong*
program. This reshapes the harness:

1. **The bug class is "accepted-but-divergent," not "panic" or "taint bypass."**
   A fuzzer that only asserts "doesn't crash" or "tainted data can't reach exec"
   would never have flagged these. → The **primary oracle must be differential
   against bash**: for input `I`, either sash *rejects* `I` with a diagnostic,
   OR `sash(I)` runtime output ≡ `bash(I)` output. Any accepted-and-divergent
   `I` is a finding. (`sash/src/oracle.rs` + `oracle_tests.rs`, ~72 cases today,
   already encode this contract — that harness is the ready-made differential
   backend; cargo-fuzz just needs to drive inputs into it.)

2. **Fuzz the whole front-end, not just the validator.** The defect was in the
   **parser** (`parse_parameter_expansion`'s silent fallthrough), *upstream* of
   the validator — the validator never saw the dropped bytes. A validator-only
   harness misses parser silent-drops by construction. Drive input through
   parse → validate → type_check → codegen → run, oracle at the end. (Retitled
   from "validator" to "front-end" for this reason.)

3. **Grammar-aware generation finds fallthroughs fast.** The holes lived in
   mini-grammars with a catch-all default: `${name OP word}`, `${a[idx]OP}`,
   `$(( … ))`. A structure-aware generator emitting `${` + name + random
   operator bytes + `}` (and the subscript / arith analogues) hits `${x&y}` /
   `${x:foo}` immediately; flat byte fuzzing wastes most of its budget never
   producing a brace-balanced construct.

4. **Assert the no-ambiguity invariant.** Post-fix, every `${…}` is either fully
   recognised or rejected (SB4034 known-dropped / SB4047 unknown). The fuzzer
   can assert the meta-property directly: *accepted ⟹ bash-equivalent*. The
   SB4047 sentinel means the accepted corpus is exactly the "understood" inputs
   — the set worth oracle-checking. A regression that reintroduces a silent
   fallthrough shows up as an accepted-and-divergent input.

5. **Highest-yield targets = every `_ =>` default that isn't a hard reject.**
   Parameter expansion alone hid 2+ silent-drop families. The same shape —
   operator/disposition dispatch with a permissive fallthrough — recurs in the
   arith operator table, `[[ ]]` operators, redirect targets, and subscript
   parsing. Enumerate these `_ => <default>` sites first; each is a candidate
   silent-degrade hole.

## Method note

Manual intuition (DG) + the coverage grind found these one at a time; the point
of this task is to **automate** the class. Suggested build order:
(a) wrap `oracle.rs` behind a `cargo-fuzz` target that maps bytes → sash source;
(b) start with a grammar-aware generator for the param-expansion / subscript /
arith mini-grammars; (c) treat "sash accepts && output ≠ bash" and "sash panics"
as the two failure conditions; (d) minimise + fold each finding into a spec.

**Open (DG decision before building):** differential-vs-bash as the primary
oracle (recommended) vs. a cheaper accept/reject-only property; grammar-aware
vs. coverage-guided byte fuzzing (or both); whether to gate `mk test` on it.

## Verified 2026-09-12 — nothing built, and the premise was just confirmed

No harness exists: no `sash/fuzz/`, and no `cargo-fuzz` / `proptest` /
`arbitrary` dependency in `sash/Cargo.toml`.

The filing's central argument — that the dangerous class is
**accepted-but-divergent**, not panics or taint bypasses — was independently
confirmed this session. `[[ -N f ]]` compiles cleanly and returns the wrong
answer versus bash (`bug-file-test-n-diverges-from-bash`), found by exactly
the differential-vs-bash method this task proposes, run by hand over one
operator family. A crash-only or taint-only fuzzer would not have flagged it.
