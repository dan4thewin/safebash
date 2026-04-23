# safebash — TODO

## Completed

- **Phase 1** — Toolchain bootstrap (001–005)
- **Phase 2** — Safe subset validation (006–011), 7 rules
- **Phase 3** — IR construction + printer (012–015), 14 stmt types, 10 expr types
- **Phase 4** — SSA transformation (016)
- **Phase 5** — Janet oracle (017)
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
- sash-rt pure Rust runtime (rustix, no libc, 84 runtime tests)
- sash-expand standalone KVP config loader (DD#1)
- DD#34 safe inner functions, DD#26 overflow traps
- DD#36 fixed-point dtoi/itod
- DD#37 reject process substitution, coerce to pipe-while-read
- DD#38-39 feature matrix review, glob → star-only + regex mutation
- DD#40 shellcheck as default lint gate, validator → 4 subset rules
- DD#41 sash-native array syntax (@arr, $arr[i], len, keys, join)
- DD#42 env vars + positional params as builtins (getenv/argv)
- DD#43 revised bless semantics — assign then validate
- DD#44 taint quarantine — full enforcement at all use sites
- 44 design decisions documented in JOURNAL.md
- ~297 tests (230 codegen + 59 oracle + 68 validator/taint)

### Features implemented (session 2026-04-19 – 2026-04-21)

- `[[ ! expr ]]` negation (negated unary + negated binary)
- `for x in "${arr[@]}"` array iteration via table index loop
- `${!arr[@]}` key iteration
- `readonly` compile-time enforcement
- `read -r a b c` multi-variable word splitting
- `read -d DELIM` custom delimiter
- `read -d: -a arr` split into array by delimiter
- Sash-native array syntax: `@arr`, `$arr[i]`, `len`, `keys`, `join`
- `for x in $(cmd)` silently coerced to pipe-while-read
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
- `unset map[key]` — Python-style tombstones in sash_table
- Nested arithmetic parens `$(( (a+b)*c ))` via AST walking
- Validator rebuilt: 4 subset rules, coercion rules removed
- Taint tracking: flow-sensitive, 68 tests
- `bless VAR EXPR` syntax

### Near-term: kusto compilation target — ✅ COMPLETE

### Near-term: unittest compilation target (LSG-linux-yocto)

- **Preprocessor**: inline `source`/`.` targets at compile time
- **`local fn` declarations** (DD#18): lexically scoped function shadowing
- `local -n` / `declare -n` namerefs — allowed per DD#3, needs codegen
- ~~`exec fd>&fd`~~ ✅ standalone redirects
- Glob patterns in `[[ ]]` — `[[ $x = */OS_A ]]` (pattern match)
- ~~`(( arithmetic ))` as statement~~ ✅ (exp 041)
- Shell options (`set -eu`, `shopt`) — compile-time feature gating
- ~~`$EPOCHSECONDS`~~ ✅
- TAP protocol output (`echo "ok $n" / "not ok $n"`)

### Near-term: soc-os-update-flash.sh compilation target

- ~~`${var:offset:length}` substring~~ ✅
- ~~`while read -r var` from pipe~~ ✅
- ~~`read -d` / `mapfile`~~ ✅
- ~~Pipeline + `while read` + `lastpipe`~~ ✅
- ~~`command -v cmd`~~ ✅ / ~~`command cmd args`~~ ✅
- ~~Runtime glob expansion~~ ✅ (star glob + regex filter)
- ~~`${#array[*]}` array length~~ ✅
- `printf -v var` — coerce to `var=$(printf ...)` (low priority)
- `getopts` — coerce to `while/case/shift` (low priority)
- `local -` — scoped shell options
- `$EPOCHREALTIME` — microsecond timestamp
- `define_dumpimage` — runtime fn redefinition (needs `local fn` or `if/else`)
- `eval 'func() { ... }'` — dynamic fn creation (needs refactoring)

### Integration work

- **Wire getenv/argv builtins into codegen** — validator expects them (DD#42)
- **Connect validator (045) with compiler (044)** — validation before compilation
- **Shellcheck integration** — `sash` runs shellcheck by default (DD#40)
- **Unified `sash` crate** — consolidate experiments into Cargo workspace

### Testing

- Oils spec test harness — ingest, filter to safe subset, run against compiler
- Codegen regression test for integer overflow (DD#26)
- Bash source test suite — after `source`/`.` is implemented
- Adversarial audit run-2 (after significant new code)

### Long-term

- Binary signing pipeline
- IPE / fs-verity integration
- Cranelift JIT for REPL
- aarch64 target testing

## Out of scope

- `select` loops (interactive)
- Coproc (concurrency)
- `eval` (incompatible with AOT)
- `<(...)` / `>(...)` process substitution (DD#37)
- `declare -f` / `unset -f` (replaced by `local fn`)
- Dynamic `source`/`.` (replaced by preprocessor)
