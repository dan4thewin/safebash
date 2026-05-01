# Experiment 030: Janet oracle parity

**Date:** 2026-04-14

**Goal:** Bring the Janet oracle current with experiments 028–029 and
reframe its role in the project.

**Design Decision #15: The Janet oracle defines safebash semantics.**

Safebash is not a bug-for-bug reimplementation of bash. It compiles a
safe subset — and may refuse functionality or replace it with a safe
alternative, so long as the alternative does what the programmer
*intended*, safely. Bash itself is not the specification. The *model*
is the specification.

Testing against bash checks behavior for a given set of inputs. The
Janet oracle externalizes the *reasoning* — the emitted Janet is a
legible, inspectable statement of what each bash construct *means*.
`(string/slice x 0 (string/find "/" x))` is a readable claim about
what `${x%%/*}` does. If that claim is wrong, you can see it by
reading, even without running.

When the oracle and bash disagree, the question is not "which is
right?" but "which behavior does safebash want, and why?" Every such
disagreement is a design decision worth documenting.

Two independent implementations agreeing is epistemically stronger than
one implementation matching a reference. If codegen has a bug that
happens to produce correct output for test inputs, the oracle catches
it — because it would need the *same* bug independently. The oracle
can't share Cranelift bugs with the codegen because it doesn't use
Cranelift at all.

**Changes to emit_janet.rs:**
- `%%` / `%` (suffix stripping): glob patterns with `*` at either end
  → `string/find` or `string/find-all` + `string/slice`
- `##` / `#` (prefix stripping): same glob handling → `string/find`
  for shortest, `string/find-all` for longest
- `:-` (default value): improved to use `(if (= (length var) 0) ...)`
  for declared variables, rather than `(or ...)` which treats empty
  string as truthy in Janet
- Literal suffix/prefix patterns → `string/has-suffix?` /
  `string/has-prefix?` + `string/slice`

**Changes to ssa_printer.rs:**
- Updated to handle all new SSA value types: `FuncCall`, `CmdSubst`,
  `Concat`, `ParamExpansion`, `Test`

**Changes to ir.rs, lower.rs, ssa.rs, transform.rs:**
- Brought forward from experiment 029 (current IR/SSA/transform)

**Changes to GUIDE.md:**
- Rewrote Janet Oracle section to reflect DD#15 — oracle as normative
  semantics, not just a test tool

**Tests:** 38 oracle tests (28 original + 10 new):
- `oracle_default_value_empty` — `${x:-fallback}` when unset
- `oracle_default_value_set` — `${x:-fallback}` when set
- `oracle_strip_suffix_longest` — `${x%%/*}` → `foo`
- `oracle_strip_suffix_shortest` — `${x%/*}` → `foo/bar`
- `oracle_strip_prefix_shortest` — `${x#*/}` → `bar/baz`
- `oracle_strip_prefix_longest` — `${x##*/}` → `baz`
- `oracle_cmdsub_echo` — `x=$(echo hello)` → `hello`
- `oracle_cmdsub_with_args` — `x=$(uname -s)` → matches bash
- `oracle_cmdsub_inline` — `echo "$(echo world)"` → `world`
- `oracle_cmdsub_with_variable` — `$(echo "$name")` with interpolation

