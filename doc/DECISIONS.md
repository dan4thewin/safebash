# Design Decisions — Quick Reference

*Condensed from `JOURNAL.md`. Shows final state of each decision,
incorporating all amendments. Grouped by topic.*

---

## Safety & Taint

**DD#24/43/44: Taint quarantine and bless.**
All external input is tainted: `$(cmd)`, `$(getenv VAR)`, `$(argv N)`,
`read`. Tainted variables cannot be used at any site — not just
dangerous ones — until blessed. Bless syntax: `bless var [[ condition ]]`
(assign first, validate second). `(( ))` and `$(( ))` are read
contexts — tainted vars in arithmetic are compile errors. Auto-bless
via `known_externals` for 26 env vars and 25 commands with conservative
regex patterns.

**DD#37: Reject process substitution `<(cmd)`.**
Cannot be statically analyzed. Use explicit temp files or pipes.

**DD#38: Reject subshells. `&` is the fork mechanism.**
`( cmd )` rejected. Background execution via `cmd &`. Fork is
explicit, never implicit.

**DD#23: PATH is compile-time only.**
Scripts cannot modify PATH at runtime. External command lookup is
resolved at compile time. Eliminates PATH injection.

---

## Rejected Constructs

| Construct | Rule | Alternative |
|---|---|---|
| `eval` | SB4001 | Restructure logic |
| `source` / `.` | SB4002 | Safe loader (deferred) |
| `export` / `declare -x` | SB5001 | `FOO=bar cmd` |
| `$(cmd)` in heredoc body | SB5002 | Extract to var, bless, reference |
| `${!var}` indirect expansion | Rejected | Direct reference |
| `select` | Out of scope | — |
| Coproc | Out of scope | — |
| `<(cmd)` process substitution | DD#37 | Pipes, temp files |
| Subshells `( cmd )` | DD#38 | `cmd &` for backgrounding |

---

## Type System & Declarations

**DD#30: Strict type separation.**
No implicit conversion between int and string. Conversion happens
explicitly at boundaries: `$(( ))` for string→int, `"$var"` for
int→string (at output).

**DD#35/45: Type keywords and declaration axes.**
Four orthogonal axes, freely stackable:

| Axis | Options |
|---|---|
| Mutability | `const` (compile-time enforced) or mutable |
| Lifetime | `local` (function-scoped) or global |
| Type | `int` or string (default) |
| Structure | `array`, `map`, or scalar (default) |

Examples: `int x=5`, `const msg="hello"`, `local const int n=0`,
`array arr=(1 2 3)`, `map m=([a]=1)`.

`const` applies to scalars only. Arrays and maps are always mutable.

**DD#36: Fixed-point arithmetic via `dtoi`/`itod`.**
Integer-only arithmetic. Fixed-point support through explicit
scale functions (designed, not built).

**DD#16/17: Integer type system and function calls.**
`int` variables store machine-width integers. `fn name -> int { }`
for typed return. `$(func args)` is the universal call syntax.

---

## Coercions

**DD#29/45: Coerce in lowerer, not separate tool.**
One tool, one set of rules. `--emit-source` shows coerced source.
`--annotate` adds `# was: ...` comments. Same bytes go into
`.sash_source` ELF section.

| Bash | Sash native |
|---|---|
| `declare -i x=5` | `int x=5` |
| `declare -a arr` | `array arr` |
| `declare -A m` | `map m` |
| `declare -r x=5` / `readonly x=5` | `const x=5` |
| `declare -ri x=5` | `const int x=5` |
| `local -i x` | `local int x` |
| `typeset x=5` | `x=5` |
| `let "x=1"` | `(( x=1 ))` |
| `` `cmd` `` | `$(cmd)` |
| `[ test ]` | `[[ test ]]` |
| `${var-default}` | `${var:-default}` |

---

## Pattern Matching & Regex

**DD#39: Star-only glob + regex mutation.**
Globs restricted to `*` for file matching. `?` and `[...]` are
not allowed in glob context. All string mutation uses regex.

Two forms of regex replace:
- **Statement** (mutates): `var =~ s/re/str/flags`
- **Expression** (returns value): `${var:s/re/str/flags}`

Flags: `g` (global). Anchors in the regex: `^`, `$`.

`${var/pat/rep}` family accepted via glob→regex coercion:
- `${var/pat/rep}` → first match
- `${var//pat/rep}` → global
- `${var/#pat/rep}` → anchored prefix
- `${var/%pat/rep}` → anchored suffix

`${var#pat}`, `${var##pat}`, `${var%pat}`, `${var%%pat}` — prefix/suffix
strip — compile via regex internally.

Non-colon parameter expansions (`${var-x}`) silently coerced to
colon forms (`${var:-x}`). The unset/null distinction is dead in
sash — nothing is ever unset.

---

## Word Splitting & Arrays

**DD#22/28: No word splitting, no IFS.**
`$var` and `"$var"` are identical. No implicit splitting. IFS
doesn't exist. `for x in $choices` is a compile error — use
`for x in "${arr[@]}"`.

**DD#41: Sash-native array syntax.**
`array arr=(1 2 3)` and `map m=([k]=v)` as declaration forms.
`@arr` for `"${arr[@]}"` in iteration (designed, not fully wired).

**DD#22 (read taxonomy, finalized):**

| Form | Semantics | Delimiter |
|---|---|---|
| `read -r line` | One line → one var | newline |
| `read -d: -r a b c` | One line → named fields | explicit `-d` |
| `read -d: -a arr` | One line → array | explicit `-d` |
| `mapfile -t arr` | Many lines → array | newline |

`-r` always implied. `-d` replaces all `IFS=x read` patterns.

---

## Functions

**DD#27: Trampoline model.**
Functions are blocks within the single Cranelift `_start` function,
not separate Cranelift functions. Call = branch, return = branch
back. Positional params saved/restored around calls. Enables
inlining without ABI overhead.

**DD#18 (candidate): `local fn` for mocking.**
`local fn name() { ... }` — function definition scoped to enclosing
block. Replaces `eval`/`declare -f` for test mocking. Designed, not
built.

**DD#19 (candidate): Runtime function selection via SSA phi.**
Conditional function definition (`if ... then f() { A } else f() { B }`)
compiled as two concrete functions with phi-based dispatch. Designed,
not built.

---

## Shell Options & Builtins

**DD#20: Shell options are compile-time decisions.**
No `shopt` or `set` commands at runtime. `extglob` is always on
(superseded by DD#39: no extglob, star-only). `nullglob` is always
on. `lastpipe` is always on. `set -e` and `set -u` semantics are
the default behavior. `set -o pipefail` is always on.

**DD#42: `getenv` and `argv` as builtins.**
`$(getenv HOME) || exit 1` replaces `$HOME`. `$(argv 1) || exit 1`
replaces `$1` at script level. Designed, partially implemented —
`$HOME` and `$1` still work via compatibility layer.

**DD#21: `-` prefix for error suppression.**
`- cd /tmp` suppresses cd failure (equivalent to `cd /tmp || true`).
Syntactic sugar for the `|| true` pattern.

---

## Compiler & Runtime

**DD#12: Null-terminated counted strings.**
Strings carry both a pointer and a length, but are also
null-terminated for C interop. Best of both worlds.

**DD#25: Arithmetic overflow traps and call depth limit.**
Integer overflow traps at runtime (not silent wraparound).
Call depth limited to prevent stack exhaustion.

**DD#26: FreeRTOS heap_4 allocator.**
Bump allocator from FreeRTOS, compiled from C. Fast, no
fragmentation for the append-heavy workload of shell scripts.

**DD#33: Unified hash table for arrays.**
Single `SashTable` type for both indexed and associative arrays.
Indexed arrays use integer keys internally. Simplifies runtime.

**DD#34: Minimize unsafe in sash-rt.**
Runtime is `#![no_std]` with rustix for syscalls. Unsafe limited
to: entry point `_start`, allocator, raw syscalls. All string
operations in safe Rust.

**DD#32: Honest trace + embedded source (not built).**
`SASH_TRACE=1` emits evaluated conditions and branch outcomes to
stderr at runtime. `.sash_source` ELF section embeds the sash-native
source for extraction. The ELF section is implemented; the runtime
trace is not.

**DD#45: Source emitter architecture.**
Lowerer records coercions during CST walk. Source emitter copies
original bytes for unchanged regions, substitutes coerced text
where needed. `--emit-source` and `.sash_source` ELF section
produce identical output.

---

## Miscellaneous

**DD#10: `bash -c` rejected.** Dynamic code execution.

**DD#11: Aliases rejected.** Textual substitution, incompatible
with static analysis.

**DD#13: Keep `ld` as linker.** Battle-tested, audited, no reason
to replace.

**DD#14: Composable process primitives.** Fork, exec, wait, pipe
as building blocks. All higher-level constructs (pipelines,
subshells, command substitution) composed from these.

**DD#15: Janet oracle defines semantics.** The Janet backend is the
reference implementation for what sash programs should do.
Discrepancies between native and Janet output are bugs.

**DD#31: `(( ))` mutations and comparisons are disjoint.**
Assignment in `(( ))` is a statement. Comparison in `(( ))` is
used only as a condition. No mixing `(( x = y == z ))`.

**DD#40: Shellcheck as lint gate.**
Shellcheck runs first (skippable with `--nolint`). Validator rules
handle sash-specific checks (SB codes). Shellcheck scope reduced —
many SC rules irrelevant because sash eliminates the underlying
hazard (e.g., word splitting).

---

## Status Key

| Status | Meaning |
|---|---|
| ✅ Implemented | In compiler, tested |
| 🔧 Partially implemented | Core works, edges missing |
| 📐 Designed | Documented, not built |
| ❓ Candidate | Proposed, not committed |

| DD | Status |
|---|---|
| 1-9 (early decisions) | ✅ mostly, DD#3 namerefs 📐, DD#8 JIT coerce 📐 |
| 10-11 (bash -c, aliases) | ✅ |
| 12 (counted strings) | ✅ |
| 13 (ld linker) | ✅ |
| 14 (process primitives) | ✅ |
| 15 (Janet oracle) | ✅ |
| 16-17 (int type, call syntax) | ✅ |
| 18 (local fn) | ❓ |
| 19 (SSA phi functions) | ❌ YAGNI — use const int flag + branch |
| 20 (shell options) | ✅ |
| 21 (- error suppression) | 📐 |
| 22 (no word splitting) | ✅ |
| 23 (compile-time PATH) | ✅ |
| 24/43/44 (taint/bless) | ✅ |
| 25 (overflow traps) | 📐 |
| 26 (heap_4 allocator) | ✅ |
| 27 (trampoline functions) | ✅ |
| 28 (no word-split loops) | ✅ |
| 29 (coerce-on-failure) | ✅ |
| 30 (strict types) | ✅ |
| 31 (arith disjoint) | ✅ |
| 32 (honest trace) | 🔧 ELF section ✅, runtime trace 📐 |
| 33 (unified hash table) | ✅ |
| 34 (minimize unsafe) | ✅ |
| 35 (type keywords) | ✅ |
| 36 (fixed-point) | 📐 |
| 37 (reject process subst) | ✅ |
| 38 (reject subshells) | ✅ |
| 39 (glob + regex) | ✅ expression form, statement form, coercion |
| 40 (shellcheck gate) | ✅ |
| 41 (array syntax) | 🔧 declaration ✅, @arr iteration 📐 |
| 42 (getenv/argv builtins) | 📐 |
| 43 (bless semantics) | ✅ |
| 44 (full taint quarantine) | ✅ |
| 45 (source emitter) | ✅ |
