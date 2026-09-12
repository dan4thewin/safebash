title: DD#15 — Janet Oracle Defines Semantics
status: superseded
superseded-by: DD104

The Janet backend is the reference implementation for what sash programs
should do. Discrepancies between native and Janet output are bugs.

Janet externalizes the compiler's reasoning in a legible, inspectable
form. `(string/slice x 0 (string/find "/" x))` is a readable claim
about what `${x%%/*}` does. If that claim is wrong, you can see it by
reading, even without running.

Two independent implementations agreeing is epistemically stronger than
one implementation matching a reference. If codegen has a bug that
happens to produce correct output for test inputs, the oracle catches
it — because it would need the same bug independently. Janet can't
share Cranelift bugs because it doesn't use Cranelift at all.

When the oracle and bash disagree, the question is not "which is
right?" but "which behavior does safebash want, and why?" Every such
disagreement is a design decision. Janet implements *sash* semantics —
when sash deliberately diverges from bash (e.g., DD#31 arithmetic),
Janet must reflect the sash behavior, not the bash behavior.

---

## Amendment (2026-08-20, DG) — the structural parse-map

DG re-aimed the janet backend: **"as near as possible to a 1-to-1 map of how
the compiler interprets the sash source — as though I'd written sash to use
sexpr instead of bash-like syntax."** The goal is *reviewability*: read the
janet beside the sash and see "oh, that didn't parse the way I expected."

This sharpens the original legibility intent from **semantic** legibility
(`${x%%/*}` → `(string/slice …)`, "what it does") to **structural / parse**
legibility (`(sash/trim-suffix …)`, "how it parsed"). Ratified decisions:

- **Structural, sash-hugging vocabulary.** Every AST node renders to a named
  `sash/*` form mirroring the construct: `sash/echo`, `sash/run`/`sash/ok?`
  (a bare *checked* command vs a *caught* condition — the position-based
  interpretation is made visible), `sash/capture`, `sash/if`/`sash/while`,
  `sash/bless`, `$name`, `sash/=`/`sash/-n`, `sash/dash` (`- cmd`), …
- **Semantics live in `sash-rt.janet`; the emitted map stays structural.**
- **Control flow: thin macros** (`sash/if` = a 1-line alias to janet `if`) so
  emit still *supplies the structure* from the IR — the IR-stage oracle
  independence (catch transform bugs) is preserved; the macro only supplies a
  readable name.
- **`bless` and similar are present and FUNCTIONAL** — a failed gate
  aborts (bare) or takes the else branch (`if bless`) — **but no clean/tainted
  flag is carried** on variables. Taint stays a compile-time concept.
- **Purely-structural map** (no taint/`checked`/type annotations); an optional
  `--annotate`-style layer can come later.
- **Built in stages** behind `--janet-map` (self-contained: the
  `sash-rt.janet` preamble is prepended), to eventually replace `--janet`.
  DG: "I prefer something that runs so I can refine by experimenting; the first
  pass doesn't need all the features."

**Reprioritization noted:** this trades a little oracle *structural*
independence (janet now mirrors the parse on purpose) for a lot of
reviewability. Independence narrows to the `sash-rt.janet` runtime (a fresh
janet implementation of the semantics), which still catches transform/codegen
bugs (janet bypasses the SSA path).
