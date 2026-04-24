# safebash — TODO

*Updated 2026-04-23. 457 tests passing (247 codegen, 114 e2e, 37 janet, 59 oracle).*
*45 design decisions documented in JOURNAL.md.*

---

## Remaining Backlog

### Safety

- Taint inheritance through bare function calls
- Adversarial review run-2 (schedule after significant new code)
- Validator rule: `alias` rejected (DD#11) — textual substitution, suggest function
- Validator rule: `set`/`shopt` table-driven (DD#20) — strip consistent options (`set -e`, `shopt -s lastpipe`, etc.), reject contradictory ones (`set +e`, `shopt -s dotglob`, etc.)
- Validator rule: `PATH=` assignment rejected (DD#23) — PATH is compile-time only. `env PATH=/dir cmd` is the escape hatch (requires `env` keyword).
- Compile-time PATH resolution (DD#23 mode 2) — `--resolve` embeds absolute paths, `--sysroot` for cross-compilation. Not yet implemented.
- Validator rule: `bash -c "literal"` rejected — remove wrapper, write code directly. `bash -c "$var"` rejected as SB4001 (dynamic execution)

### Compilation — High Priority

- Per-command env vars for externals — `FOO=bar cmd` (externals only, not functions). This is the mechanism that replaces `export` (SB5001).
- ~~Case exit status propagation from matched arm~~ — fixed (c36769d)
- Compound command redirects — `for...done > file`, `for...done | cmd`
- Heredoc with `$(cmd)` — lowerer doesn't expand command substitutions in heredoc bodies (variable expansion works)

### Compilation — Normal

- Arithmetic modulo at runtime (constant-folded works)
- Namerefs — `local -n ref=target` / `declare -n` (DD#3). Compile-time substitution in lowerer during inlining. Argument must be bare identifier, not computed string.

### Language Features

- Honest trace (DD#32) — `SASH_TRACE=1` runtime value tracing, designed not built
- `join` builtin (DD#41) — `join @arr`, `join sep @arr`. Replaces `"${arr[*]}"` IFS joining.
- `@arr` iteration syntax (DD#41) — shorthand for `"${arr[@]}"`
- `getenv`/`argv` builtins (DD#42) — `$(getenv HOME)` / `$(argv 1)` as explicit boundary
- `-` error suppression (DD#21) — `- cd /tmp` as sugar for `cd /tmp || true`
- `local fn` for mocking (DD#18 candidate) — lexically scoped function shadowing
- Fixed-point arithmetic (DD#36) — `dtoi`/`itod` scale functions
- Arithmetic overflow traps (DD#25) — runtime trap on integer overflow
- Pike regex shorthand classes (`\d \w \s`)

### Testing

- Brush test suite: 173 PASS, 78 FAIL_OUTPUT, 43 FAIL_COMPILE (baseline from April 22, needs re-run)
- Oils test suite: needs systematic runner

### Tooling

- `--emit-source` coercion for `${var/pat/rep}` → `${var:s/re/str/}` (grammar wired, lowerer needs coercion recording)
- `--emit-source` coercion for `"${arr[*]}"` → `$(join @arr)` (after join is implemented)

### Documentation

- SELF 2026 talk prep

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
