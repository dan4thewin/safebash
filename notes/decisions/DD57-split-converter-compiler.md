title: DD#57 — Split Converter and Compiler
status: implemented

> **Implemented (status updated 2026-06-21).** The split is done:
> `sash-convert` is a separate crate on **stock** `tree-sitter-bash` (runs
> shellcheck, parses findings, emits `# CHECK:` hints); `sash` is the
> compiler. **Update:** the line below about the compiler "keeping the
> modified fork (future: purpose-built sash-native parser)" was overtaken
> — the compiler now uses a **hand-written parser** and dropped tree-sitter
> entirely (commit 1c0e64a); the fork is no longer part of the build. Only
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

## Addendum (2026-10-25) — an int operand of a converted test is quoted

DD#30's addendum of the same date makes a whole-int `[[ ]]` operand SB4030.
Bash has no such line, so without this every converted test on an int would
now fail to compile. The converter now quotes, as a mechanical conversion, each whole
test operand that the converted script holds as a sash int: `$(( … ))`; `$?`,
`$$`, `$!` and `$#`, which convert to `$(status)`, `$(getpid)`, `$(getcpid)`
and an arithmetic count; a length `${#…}`; and a read of an int-typed name.
Operands of the `-eq` family are left alone, because those tests are rewritten
to `(( ))`.

Which names are int-typed is a name-based **superset**, found by a pre-pass:
`declare -i`/`local -i`, every arithmetic write target (`(( k = 3 ))`,
`let j=4`, `k++`, a C-style `for` variable), and, to a fixpoint, every copy of
an int expansion (`rc=$?`, `n=$(( … ))`, `q=$p`). A superset is safe because
quoting a whole operand never changes what a sash test means, while a name it
misses is SB4030 at compile time: loud, never silent. The existing `[ ]` →
`[[ ]]` quote-stripping keeps the quotes on an int-typed name.

Two pre-existing faults had to be fixed with it. The converter treated a
C-style `for` body, and anything inside a command substitution within
arithmetic, as arithmetic, so a `$?` there became the DD#58 sexpr `(status)`
and a string read gained an integer abort disposition. Unquoted, that
`[[ (status) == $e ]]` was a loud SB4068; quoted, it would have compiled and
compared `$e` against the literal text `(status)`. Only a `for`'s header is
arithmetic now, and a command substitution's body never is. `--annotate`
writes its `# was:` comment after the added closing quote, so the comment
cannot become part of the operand. A mid-line annotation still comments out
the rest of its line, which is pre-existing
(`notes/tasks/bug-convert-annotate-midline.md`).

Specs `spec/convert.sh` `57.178`-`57.191`; journal 535.
