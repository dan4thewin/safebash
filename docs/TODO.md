# safebash — TODO

*Updated 2026-04-25. 502 tests passing (247 codegen, 116 e2e, 37 janet,
59 oracle, 43 taint spec). 51 design decisions in JOURNAL.md.
CST taint gate + SSA taint backstop (DD#51). Adversarial audit run-2
complete (5 models, 12 findings, all remediated).*

---

## Safety & Taint Model

**Architecture (DD#51, 2026-04-25):** Two-layer taint enforcement.
CST analyzer (`taint.rs`, ~750 lines) is the primary gate — runs before
lowering, gives precise source-location diagnostics. SSA verifier
(`ssa_taint.rs`, ~300 lines) is the backstop — runs after transform,
before codegen. If the SSA pass fires, that's a bug in the CST pass.
This replaced the "re-platform taint on SSA" plan; defense-in-depth
won over migration.

### Remaining safety work

- **Bash-mode positional coercion** (DD#46 remainder) — top-level
  `$1..$9`, `$@`, `$*` should be coerced to `$(argv N)` / `$(argv)` in
  bash-compatibility mode. Sash-native mode already rejects them.
- **Fuzz target for validator** — no `cargo-fuzz` or property-test
  harness. Needed to find bypasses the audit didn't.
- **Validator rule: `alias`** (DD#11) — textual substitution; suggest
  function.
- **Validator rule: `set`/`shopt` table-driven** (DD#20) — allow
  consistent options (`set -e`, `shopt -s lastpipe`), reject
  contradictory ones.
- **Validator rule: `PATH=` assignment** (DD#23) — PATH is compile-time
  only. `env PATH=/dir cmd` is the escape hatch.
- **Validator rule: `bash -c "literal"`** — reject with guidance to
  remove the wrapper. Dynamic form already caught by SB4001.
- **Compile-time PATH resolution** (DD#23 mode 2) — `--resolve` embeds
  absolute paths, `--sysroot` for cross. Not yet built.

## Compilation — High Priority

- **Per-command env vars for externals** — `FOO=bar cmd`. Documented
  replacement for rejected `export` (SB5001); without it that rejection
  has no escape hatch.
- **Compound command redirects** — `for…done > file`, `while…done | cmd`,
  `if…fi > file`. Codegen handles redirects per simple command only.
  `codegen.rs` has an explicit `// TODO: other redirect types` marker.
- **Heredoc with `$(cmd)`** — lowerer doesn't expand command
  substitutions in heredoc bodies (variable expansion works). SB5002
  catches the dangerous case but the safe coerced path isn't built.

## Codegen — Convert Panics to Diagnostics

Each is a UX cliff. Either implement or surface as a `Diagnostic`
upstream so codegen never sees them.

- `panic!("unimplemented test operator")` for binary `[[ ]]` operators
- Unary test `eprintln!` with no exit — silently wrong code
- `panic!("non-constant elidable function call not yet supported")`
- `// TODO: other redirect types` in redirect dispatcher
- `unset -f func` silently ignored (should diagnose; `local fn` planned)
- `continue` in Janet emitter via fallthrough (needs `when`/`unless`)
- The `panic!("undefined …")` cluster (~10 sites) — upstream passes let
  something through that codegen assumed unreachable

## Compilation — Normal

- Arithmetic modulo at runtime (constant-folded form works)
- Namerefs — `local -n ref=target` / `declare -n` (DD#3). Compile-time
  substitution during inlining.

## Language Features (designed, not built)

- **`join` builtin** (DD#41) — `join @arr`, `join sep @arr`
- **`@arr` iteration syntax** (DD#41) — shorthand for `"${arr[@]}"`
- **`getenv`/`argv` as first-class builtins** (DD#42) — codegen supports
  them but they're not exposed as standalone commands yet
- **`-` error suppression** (DD#21) — `- cmd` as `cmd || true`, plus
  pipeline coercer for non-pipefail scripts
- **`local fn`** (DD#18 candidate) — lexically scoped function shadowing
- **Honest trace** (DD#32) — `SASH_TRACE=1` runtime value tracing
- **Fixed-point arithmetic** (DD#36) — `dtoi`/`itod` shipped; full ops not
- **Arithmetic overflow traps** (DD#25) — partial; not fully wired
- **Pike regex shorthand classes** — `\d \w \s` in `${var:s/…/…/}` and `=~`

## Testing

- **Brush suite** — 173 PASS, 78 FAIL_OUTPUT, 43 FAIL_COMPILE (baseline
  2026-04-22, needs re-run and triage)
- **Oils suite** — harness exists (`suites/oils-harness.sh`,
  `oils_runner.py`), not integrated into `mk test`
- **Adversarial audit run-3** — schedule after next significant feature
  work. Run-2 artifacts in `audit/run-2/`.

## Tooling

- `--emit-source` coercion for `${var/pat/rep}` → `${var:s/re/str/}`
- `--emit-source` coercion for `"${arr[*]}"` → `$(join @arr)` (after join)
- `cfp` — undocumented script in repo root; document or remove

## Documentation

- SELF 2026 talk prep
- DECISIONS.md vs JOURNAL.md drift — reconcile (DECISIONS.md is 305 lines,
  JOURNAL.md has DD#1–51 with amendments)

---

## Suggested Next-Up Order

1. **Per-command env vars** — unblocks the `export` replacement story
2. **Compound-command redirects** — common bash idiom, currently broken
3. **Convert codegen panics to diagnostics** — each is a UX cliff
4. **`getenv`/`argv` + `@arr`/`join`** — docs ahead of implementation
5. **Brush re-run** — measurement before more building

---

## Explicit non-goals

- Inter-procedural taint inference / function signatures (DD#24)
- `select` loops (interactive)
- Coproc (concurrency)
- `eval` (incompatible with AOT)
- `<(…)` / `>(…)` process substitution (DD#37, now SB5004)
- `declare -f` / `unset -f` (replaced by `local fn`)
- Dynamic `source`/`.` (replaced by preprocessor)
- `$((x=1))` as expression (DD#31)
- Recursive arithmetic `$((z))` where z is string (implicit eval)
- `${arr[@]/pat/rep}` per-element replacement (use loop)
- DD#8 JIT coerce mode (far-future)
- DD#10 `bash -c` inlining (YAGNI)
- DD#19 SSA phi function dispatch (YAGNI)

---

## Completed (newest first)

- **DD#51** SSA taint verification pass — defense-in-depth backstop
- **DD#50** Function free-read tracking — inter-procedural call-site check
- **DD#49** Tautological bless rejection (SB4012)
- **DD#48** Out-of-range positional detection
- **DD#47** Bless runtime enforcement (codegen evaluates test, aborts on fail)
- **Audit run-2** — 5 models, 12 findings (T1–T12), all remediated
  - T1: herestring/heredoc false clean
  - T2: tautological bless
  - T3: bless without valid `[[ ]]`
  - T4: arithmetic assignment taint laundering
  - T5: parameter expansion operand taint
  - T6: function global re-taint at call site
  - T9: shift + positional range
  - T10: direct `$(getenv)` in expression
  - T11: case pattern taint
  - T8: nameref rejected (SB5003)
  - T12: process substitution rejected (SB5004)
- **DD#46** Positional params function-local (top-level rejection)
- **DD#40–45** Validator rebuild, taint quarantine, bless, grammar fork
- **DD#34–39** Safe inner functions, overflow traps, fixed-point,
  process-sub rejection, feature matrix, glob→regex
- **DD#28–33** Pipe-while-read, type system, concat, shift, export
- **DD#14–27** Cranelift AOT, Janet oracle, command substitution,
  parameter expansion, composable process primitives
- **DD#1–13** Toolchain bootstrap, safe subset validation, IR, SSA
- Full feature list: see `git log` and JOURNAL.md
