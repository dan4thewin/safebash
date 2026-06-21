title: DD#57 — Split Converter and Compiler
status: implemented

> **Implemented (status updated 2026-06-21).** The split is done:
> `sash-convert` is a separate crate on **stock** `tree-sitter-bash` (runs
> shellcheck, parses findings, emits `# CHECK:` hints); `sash` is the
> compiler. **Update:** the line below about the compiler "keeping the
> modified fork (future: purpose-built sash-native parser)" was overtaken
> — the compiler now uses a **hand-written parser** and dropped tree-sitter
> entirely (commit 73caecf); the fork is no longer part of the build. Only
> `sash-convert` uses tree-sitter.

Two programs: `sash-convert` (bash → sash-native source) and `sash` (sash-native → binary).
Motivated by parser divergence — one tree-sitter fork serving two languages accumulates
syntax compromises. The converter uses unmodified upstream tree-sitter-bash. The compiler
keeps the modified fork (future: purpose-built sash-native parser).

Shellcheck is an internal analysis tool, not a precondition. The converter runs shellcheck
on the input, consumes its JSON output, and uses specific SC codes to distinguish mechanical
conversions from ambiguous intent.

## Conversion categories

### Mechanical — convert silently

| Bash construct | Sash-native output | Notes |
|---|---|---|
| `declare -i x=5` | `int x=5` | |
| `declare -a arr=(...)` | `array arr=(...)` | |
| `declare -A m=(...)` | `map m=(...)` | |
| `declare -r x=5` / `readonly x=5` | `const x=5` | |
| `declare -ri x=5` | `const int x=5` | Composite flags |
| `local -i x=5` | `local int x=5` | |
| `typeset x=5` | `x=5` | Bare assignment |
| `let "expr"` | `(( expr ))` | SC2219 confirms; convert regardless |
| `` `cmd` `` | `$(cmd)` | SC2006 confirms; convert regardless |
| `[ expr ]` | `[[ expr ]]` | SC2292 optional; convert regardless |
| `${var-x}` | `${var:-x}` | Shellcheck doesn't flag; always convert |

### Ambiguous — emit `# CHECK:` hint

| Bash construct | SC code | Ambiguity |
|---|---|---|
| Unquoted `$var` in command args | SC2086 | Could be intentional word splitting. In sash, no splitting occurs. If the script relied on splitting, behavior changes silently. Converter emits `# CHECK: $var is unquoted — word splitting won't occur in sash. Was splitting intended?` |

### Manual — emit `# MANUAL:` annotation, pass through unchanged

| Bash construct | Why |
|---|---|
| `eval "..."` | Dynamic execution. No mechanical conversion. |
| `source` / `.` | File inclusion. Sash has no source; use sash-expand pipeline (DD#54). |
| `export VAR=val` | No export in sash (DD#53). Env isolation by design. |
| `${!var}` indirect expansion | Pointer-like indirection. No sash equivalent. |
| `declare -n` / nameref | Alias indirection. No sash equivalent. |
| `<(...)` / `>(...)` process substitution | No sash equivalent. |
| `${var@Q}` / `${var@E}` etc. | Parameter transformations rejected. |
| Heredoc with `$(cmd)` inside | Untraceable taint (SB5002). Extract to variable + bless. |

### Best-effort bless — emit `bless` line or `# BLESS:` hint

| Pattern | Action |
|---|---|
| `x=$(date +%F)` and similar known externals | Emit `bless x [[ $x =~ pattern ]]` with the known pattern from the externals list |
| `x=$(somecommand)` where command is not in known list | Emit `# BLESS: x is assigned from 'somecommand' — add a bless with appropriate pattern` |
| `read x` | Emit `# BLESS: x is read from stdin — add a bless with appropriate pattern` |

## Acceptance criteria

1. `sash-convert` produces valid sash-native source from bash input
2. All mechanical conversions produce correct sash-native syntax
3. Ambiguous cases (SC2086) are annotated with `# CHECK:` hints
4. Unconvertible constructs are annotated with `# MANUAL:` and left unchanged
5. Known externals get real `bless` lines; unknown externals get `# BLESS:` hints
6. `--annotate` adds `# was: original` comments to mechanical conversions
7. `sash-convert foo.sh | sash -o binary` produces working binaries for scripts in the safe subset
8. `sash` compiles sash-native source without any bash validation rules
9. Existing test suite passes (tests adapted to use sash-native input where needed)
10. shellcheck is invoked internally, not required as a precondition
