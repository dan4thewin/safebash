# safebash — TODO

*Updated 2026-04-25. 502 tests passing (247 codegen, 116 e2e, 37 janet, 59 oracle, 43 taint spec). SSA taint backstop active..*
*51 design decisions documented in JOURNAL.md.*
*2026-04-24: repo-wide gap survey expanded the backlog (codegen panics,
pipeline `-` coercer, fuzzing, oils harness, doc drift).*

---

## Remaining Backlog

### Safety

- **DD#46: positional params are function-local only** (2026-04-24).
  ~~Sash-native mode: top-level `$1..$9`, `$@`, `$*`, `$#` are a
  compile error; use `argv N` / `argv` / `len argv`.~~ **Done** (top-level
  rejection implemented). ~~Both modes: reading `$K` where `K > $#`
  (call's actual arg count) is a compile error.~~ **Done** (DD#48,
  2026-04-24). **Remaining:** bash-mode coercion of top-level positional
  refs to `$(argv N)` — not yet implemented.

- **Re-platform taint enforcement on SSA** (DD#24 amendment 2,
  2026-04-24 — see JOURNAL). The CST analyzer in `taint.rs` was
  built around a wrong mental model (flow-sensitive with reassignment
  re-taint) and has accumulated bugs from reimplementing scope and
  dataflow that SSA already computes. **Partially done** (2026-04-24):
  CST taint now handles arithmetic, subscript, herestring, user-func
  capture correctly. Full SSA re-platform still pending.
  architecture.
- **Known bypass in current CST analyzer**: a global pre-declared
  clean and overwritten tainted inside a function body compiles
  without diagnostic. Will be fixed by the SSA replatform; not worth
  patching the CST version. Document in SUBSET.md as a known
  limitation until fixed.
- Adversarial review run-2 (schedule after significant new code) — overdue
  per AGENTS.md following DD#40–45 and the grammar fork. Validator-bypass
  testing is the highest-priority focus per the adversarial-audit skill.
  (One bypass already found and fixed during the 2026-04-24 survey
  without running an audit — strong signal that run-2 is ripe.)
- No fuzz target for the validator — no `cargo-fuzz` or property-test
  harness in tree. Needed to make run-2 meaningful.
- Validator rule: `alias` rejected (DD#11) — textual substitution, suggest function
- Validator rule: `set`/`shopt` table-driven (DD#20) — strip consistent options (`set -e`, `shopt -s lastpipe`, etc.), reject contradictory ones (`set +e`, `shopt -s dotglob`, etc.)
- Validator rule: `PATH=` assignment rejected (DD#23) — PATH is compile-time only. `env PATH=/dir cmd` is the escape hatch (requires `env` keyword).
- Compile-time PATH resolution (DD#23 mode 2) — `--resolve` embeds absolute paths, `--sysroot` for cross-compilation. Not yet implemented.
- Validator rule: `bash -c "literal"` rejected — remove wrapper, write code directly. `bash -c "$var"` rejected as SB4001 (dynamic execution)

### Compilation — High Priority

- **Per-command env vars for externals** — `FOO=bar cmd` (externals only,
  not functions). Documented replacement for the rejected `export`
  (SB5001); without it that rejection has no escape hatch.
- ~~Case exit status propagation from matched arm~~ — fixed (c36769d)
- **Compound command redirects** — `for…done > file`, `while…done | cmd`,
  `if…fi > file`, etc. Codegen currently handles redirects per simple
  command only. `codegen.rs:4109` has the explicit `// TODO: other
  redirect types` marker on the dispatcher.
- **Heredoc with `$(cmd)`** — lowerer doesn't expand command substitutions
  in heredoc bodies (variable expansion works). SB5002 catches the
  dangerous case but the safe coerced path (extract to blessed var) is
  not built.

### Codegen — Convert Panics to Diagnostics

Each of these is a UX cliff and a likely audit finding. Either implement
properly or surface as a `Diagnostic` upstream (validator/type-check) so
codegen never sees them.

- `codegen.rs:2465` — `panic!("unimplemented test operator")` for binary
  `[[ ]]` operators. Enumerate which operators reach this and either
  implement or reject.
- `codegen.rs:2528` — same for unary tests; currently only `eprintln!`,
  no exit, so produces silently wrong code.
- `codegen.rs:2973` — `panic!("non-constant elidable function call not
  yet supported")`. Either lift the restriction or diagnose at validate.
- `codegen.rs:4109` — `// TODO: other redirect types` (see compound
  redirects above).
- `transform.rs:478` — `unset -f func` silently ignored. Should be a
  diagnostic (function unset isn't supported; `local fn` is the planned
  replacement).
- `emit_janet.rs:271` — `continue` inside loops emitted via fallthrough;
  needs proper `when`/`unless` restructuring for the Janet oracle.
- The `panic!("undefined …")` cluster in codegen (lines 1871, 1897,
  1923, 1974, 3041, 3676, 3724, 3750, 3755, 3778) — each means an
  upstream pass let something through that codegen assumed unreachable.
  Make each unreachable-by-construction or diagnose earlier.

### Compilation — Normal

- Arithmetic modulo at runtime (constant-folded works)
- Namerefs — `local -n ref=target` / `declare -n` (DD#3). Compile-time substitution in lowerer during inlining. Argument must be bare identifier, not computed string.

### Language Features

- Honest trace (DD#32) — `SASH_TRACE=1` runtime value tracing, designed not built
- `join` builtin (DD#41) — `join @arr`, `join sep @arr`. Replaces `"${arr[*]}"` IFS joining.
- `@arr` iteration syntax (DD#41) — shorthand for `"${arr[@]}"`. Declarations
  work; iteration form does not.
- `getenv`/`argv` builtins (DD#42) — `$(getenv HOME)` / `$(argv 1)` as
  explicit boundary into the taint model. SUBSET.md uses these as the
  canonical idiom; codegen doesn't expose them as first-class builtins yet.
- `-` error suppression (DD#21) — `- cd /tmp` as sugar for `cd /tmp || true`,
  plus the per-segment **pipeline coercer** from the DD#21 amendment
  (2026-04-23): when source script lacks `set -o pipefail`, the coercer
  must insert `-` on every non-terminal pipeline segment to preserve
  bash's "only last segment matters" semantics.
- `local fn` for mocking (DD#18 candidate) — lexically scoped function shadowing
- Fixed-point arithmetic (DD#36) — `dtoi`/`itod` shipped; full fixed-point
  operators not.
- Arithmetic overflow traps (DD#25) — partial; runtime trap on integer
  overflow not fully wired.
- Pike regex shorthand classes (`\d \w \s`) in `${var:s/…/…/}` and `=~`.

### Testing

- Brush test suite: 173 PASS, 78 FAIL_OUTPUT, 43 FAIL_COMPILE (baseline
  from April 22, needs re-run and triage pass)
- Oils test suite: harness exists (`suites/oils-harness.sh`,
  `oils_runner.py`), but no systematic runner integrated into `mk test`
- Validator fuzzing — see Safety section.

### Tooling

- `--emit-source` coercion for `${var/pat/rep}` → `${var:s/re/str/}` (grammar wired, lowerer needs coercion recording)
- `--emit-source` coercion for `"${arr[*]}"` → `$(join @arr)` (after join is implemented)
- `cfp` — undocumented script in repo root; either document or remove.

### Documentation

- SELF 2026 talk prep
- DECISIONS.md vs JOURNAL.md drift — DECISIONS.md is 305 lines while
  JOURNAL.md goes through DD#45 with multiple amendments. Audit for
  agreement and reconcile.

---

## Suggested Next-Up Order (from 2026-04-24 survey)

1. **Per-command env vars** — unblocks the documented `export` replacement story.
2. **Taint inheritance through function calls** — only outright soundness gap on the list.
3. **Compound-command redirects** — common bash idiom, currently silently wrong or unsupported.
4. **Convert codegen `panic!`s into diagnostics** — every one is a UX cliff and a likely audit finding.
5. **`getenv`/`argv` + `@arr`/`join`** — SUBSET.md treats these as canonical idioms; docs are ahead of implementation.
6. **Brush re-run + adversarial audit run-2** — measurement before more building.

---

## Explicit non-goals (do not propose these)

- **Inter-procedural taint inference / function signatures.** DD#24
  rejected Perl-style propagation. The quarantine is at the program
  boundary; functions do not accept tainted parameters; bless at the
  source. Any "taint flows through call X" feature reverses DD#24.

---

## Removed from Backlog (with rationale)

- `$((x=1))` side effects — DD#31 says mutation and value production are disjoint. Use `(( x=1 )); echo $x`.
- Recursive arithmetic `z="x+y"; $((z))` — string-as-code is implicit eval. Write the expression directly.
- `${arr[@]/pat/rep}` per-element replacement — YAGNI. Use `for i in "${!arr[@]}"; do arr[$i]=${arr[$i]:s/pat/rep/}; done`.
- DD#19 SSA phi function dispatch — YAGNI. Use `const int` flag + branch, or dispatch wrapper function.

---

## Completed

- **Phases 1–5** — Toolchain bootstrap, safe subset validation (7 rules), IR construction + printer (14 stmt / 10 expr types), SSA transformation, Janet oracle
- **018** — Subshell elision analysis
- **019–020** — Adversarial audit remediation
- **021–026** — Cranelift AOT compilation, 16 KB static binaries
- **027** — Positional parameters, env vars, exit builtin
- **028** — Parameter expansion operators (`%%`/`%` `##`/`#` `:-`)
- **029** — Runtime command substitution, composable process primitives (DD#14)
- **030** — Janet oracle parity, DD#15 (oracle as normative semantics)
- **031** — Type system + grammar fork (DD#16, DD#17), type checker
- **032** — Concat, while loops, numeric tests in codegen (phi nodes, two-pass lowering)
- **033** — shift, export, $@ (sash_setenv, sash_join_args)
- **036** — sash CLI + fix redirected CmdSubst transform
- **038** — Dynamic for-in-$@ loops, DD#28 (no word-split/glob loops)
- **041** — Int vars, VarSlot, first-class `(( ))`, DD#30-31
- **043** — Unified sash_table for indexed+assoc arrays, int/map/array keywords (DD#33,35)
- **044** — Pipe-while-read (DD#28 target form)
- **045** — Validator rebuild + taint quarantine + bless (DD#40, DD#24, DD#42-44)
- sash-rt pure Rust runtime (rustix, no libc)
- sash-expand standalone KVP config loader (DD#1)
- DD#34 safe inner functions, DD#26 overflow traps
- DD#36 fixed-point dtoi/itod
- DD#37 reject process substitution, coerce to pipe-while-read
- DD#38-39 feature matrix review, glob → star-only + regex mutation
- DD#40 shellcheck as default lint gate, validator → 4 subset rules
- DD#41 sash-native array syntax (@arr, $arr[i], len, keys, join) — declaration only, @arr/join not yet wired
- DD#42 env vars + positional params as builtins (getenv/argv)
- DD#43 revised bless semantics — assign then validate
- DD#44 taint quarantine — full enforcement at all use sites
- Source emitter (`--emit-source`, `--annotate`, `--extract`) + `.sash_source` ELF section
- Declaration coercions (`declare -i`→int, `-a`→array, `-A`→map, `-r`→const, `let`→`(( ))`)
- `const` keyword + stacked modifiers (`local const int x=5`)
- Combined declaration flags (`-ri`, `-ia`)
- Export rejection (SB5001) + heredoc cmdsub rejection (SB5002)
- `${var:s/re/str/flags}` tree-sitter grammar + end-to-end compilation
- known_externals auto-bless (26 env vars, 25 commands)
- Function argument passing (`$1`/`$2` in arithmetic)
- Herestring read
- Janet backend parity (37 tests)
- Negative substring length
- Empty `case/esac`
- Glob→regex coercion for `${var/pat/rep}`
- Case `;&` and `;;&` terminators
- Heredoc `<<-` tab stripping
- Regex match `[[ =~ ]]` with variable expansion
- Pipeline negation `!`
- Fnmatch for `case`/`[[ == ]]`
- `[[ ! expr ]]` negation (negated unary + negated binary)
- `for x in "${arr[@]}"` array iteration via table index loop
- `${!arr[@]}` key iteration
- `readonly` compile-time enforcement
- `read -r a b c` multi-variable word splitting
- `read -d DELIM` custom delimiter + `read -d: -a arr`
- `for x in $(cmd)` coerced to pipe-while-read
- `command NAME args` bypass function, run external
- `$(cmd 2>/dev/null)` CmdSubst carries redirects into child
- `$(<file)` file slurp desugared to cat
- `$(func args)` stdout capture (echo→buffer)
- `$EPOCHSECONDS` coerced to epochseconds() builtin
- `exec 3>&1` standalone redirects
- `mapfile -t arr` + bare `array arr` creates empty table
- Subshells `(cmd)` — fork/wait with SSA variable isolation
- Background jobs: `cmd &`, `wait`, `$!`
- `trap 'handler' EXIT` — inlined before returns
- `trap '' SIGNAL` / `trap - SIGNAL` — ignore/reset
- Star glob: `arr=(*.sh)` via sash_readdir
- `arr=(* =~ 'regex')` — readdir with regex filter
- `var =~ s/pattern/replacement/flags` regex mutation
- `unset map[key]` — tombstones in sash_table
- Nested arithmetic parens `$(( (a+b)*c ))` via AST walking

## Out of scope

- `select` loops (interactive)
- Coproc (concurrency)
- `eval` (incompatible with AOT)
- `<(...)` / `>(...)` process substitution (DD#37)
- `declare -f` / `unset -f` (replaced by `local fn`)
- Dynamic `source`/`.` (replaced by preprocessor)
- `$((x=1))` as expression — DD#31, use `(( x=1 )); echo $x`
- Recursive arithmetic `$((z))` where z is a string expression — implicit eval
- `${arr[@]/pat/rep}` per-element replacement — use loop with `${var:s/re/str/}`
- DD#8 JIT coerce mode — far-future
- DD#10 `bash -c` inlining — YAGNI. Literal form: reject, tell user to remove wrapper. Dynamic form: SB4001.
