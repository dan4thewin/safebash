# Experiment 006: ShellCheck survey — defining the safe subset

**Goal:** Catalog ShellCheck's ~520 SC codes. Classify each by
relevance to the safebash safe subset. Produce `exp/SUBSET.md` as
the specification document for phase 2 validation.

**Actions:**
- Cloned the ShellCheck wiki (520 SC code pages) and extracted
  every code with its description
- Ran ShellCheck (0.10.0, with `--enable=all`) against synthetic
  scripts covering eval, quoting, backticks, test constructs,
  subshell scope, arrays, arithmetic, and safety patterns
- Classified codes into 9 categories, with each rule marked as
  reject (error), coerce (auto-fix), or inform (warning)

**Results:**
- 520 SC codes total: 131 SC1xxx (parse), 324 SC2xxx (code),
  65 SC3xxx (portability)
- SC1xxx and SC3xxx are out of scope (tree-sitter handles parse
  errors; portability is irrelevant for Bash 4.0+ target)
- The safe subset draws from ~50 SC2xxx codes across 9 categories:
  quoting, eval/dynamic execution, deprecated syntax, test
  constructs, subshell scope, safety/robustness, arrays, command
  safety, and arithmetic

**Key design decisions:**
- `eval` is rejected unconditionally — no coerce mode. If eval is
  allowed, the entire safe subset collapses.
- Indirect variable access (`${!var}`, `declare -n`) is also
  rejected — same reasoning as eval.
- Variable-as-command-name (`$cmd arg`) is rejected — command names
  must be literal words for static analysis.
- `[ ]` / `test` are coerced to `[[ ]]` — this eliminates an entire
  class of quoting bugs inside conditionals.
- `cd` without `|| exit` is coerced — the canonical "delete
  everything" bug.

**Conclusion:** `exp/SUBSET.md` is the specification. Phase 2
experiments (007–011) implement these rules against the CST.

---

