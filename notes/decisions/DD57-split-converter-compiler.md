title: DD#57 — Split Converter and Compiler
status: implemented

1. Keep Bash conversion and native compilation separate: `sash-convert`
   produces source; `sash` compiles the native language.
2. Convert mechanical cases; annotate ambiguous intent for review and leave
   unsupported cases with manual guidance rather than inventing semantics.
3. ShellCheck is converter-internal analysis, not a compilation precondition.
4. Generate explicit attests for known input patterns and review hints for
   unknown ones; conversion does not waive taint rules.
5. Conditional-test capture hints must explain uncaught operand failures
   without automatically suppressing them or inventing fallbacks.
6. Preserve type boundaries in converted tests and distinguish actual Bash
   expansions from literal text.

---

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
~~keeps the modified fork (future: purpose-built sash-native parser).~~
The hand-written compiler parser supersedes that plan.

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
| `local -i x=5` | ~~`local int x=5`~~ | DD86 replaces keyword lifetime with positional scope |
| `typeset x=5` | `x=5` | Bare assignment |
| `let "expr"` | `(( expr ))` | SC2219 confirms; convert regardless |
| `` `cmd` `` | `$(cmd)` | SC2006 confirms; convert regardless |
| `[ expr ]` | `[[ expr ]]` | SC2292 optional; convert regardless |
| `${var-x}` | `${var:-x}` | Shellcheck doesn't flag; always convert |

### Ambiguous — emit `# CHECK:` hint

| Bash construct | SC code | Ambiguity |
|---|---|---|
| Unquoted `$var` in command args | SC2086 | Could be intentional word splitting. In sash, no splitting occurs. If the script relied on splitting, behavior changes silently. Converter emits `# CHECK: $var is unquoted — word splitting won't occur in sash. Was splitting intended?` |
| Command capture in a conditional test operand | None required | The test handles its own result, not failures inside the capture (DD#55). Emit a review hint, not automatic suppression. Implementation queued below. |

**Conditional-test captures (DG, 2026-11-10; implementation pending).**
Emit a `# CHECK:` hint when a Bash conditional test uses a command capture
as an operand. This includes tests in `if`/`elif`, `while`/`until` and
`&&`/`||` conditions. Quoting, interpolation or negating the test does not
make the operand capture's failure its result.

The hint explains that an uncaught capture failure can abort in sash even
when Bash would continue to test the captured text. The author must review
whether to handle the capture's status or explicitly discard it where
legal. Preserve the code apart from the annotation and independently
required conversions: do not insert `-`, invent a fallback, or add a hard
conversion error.

This is not a claim that `$(- ...)` repairs every capture. DD#55's no-reach
rule keeps nested operand and called-body failures checked; result-kind
restrictions and DD#130's fatal faults remain. An actual status consumer,
such as a legal capture condition, is distinct from a capture used as a
test operand. Existing explicit handling must not be described as absent.

Taint is independent. A direct external capture in a test operand already
gets SB5020 normally, but satisfying taint requirements does not itself
handle its failure. The hint is guidance, not a promise that unchanged
output compiles or reproduces Bash behavior.

`q-convert-capture-in-condition-operand` owns implementation and focused
converter specs. Journal 609 records the ruling. Compiler semantics do not
change with this policy.

### Manual — emit `# MANUAL:` annotation, pass through unchanged

| Bash construct | Why |
|---|---|
| `eval "..."` | Dynamic execution. No mechanical conversion. |
| `source` / `.` | File inclusion. Sash has no source; use sash-expand pipeline (DD#54). |
| `export VAR=val` | No export in sash (DD#53). Env isolation by design. |
| `$-` / `${-}` option-flags expansions | Bash shell options have no sash equivalent. Manual review, not a compiler ban on literal `$-` (DD#64). Annotation queued below. |
| `${!var}` indirect expansion | Pointer-like indirection. No sash equivalent. |
| `declare -n` / nameref | Alias indirection. No sash equivalent. |
| `<(...)` / `>(...)` process substitution | No sash equivalent. |
| `${var@Q}` / `${var@E}` etc. | Parameter transformations rejected. |
| Heredoc with `$(cmd)` inside | ~~Untraceable taint (SB5002). Extract to variable + bless.~~ Heredocs are wholly retired, not conditional on their contents (DD01-09). |

**Option flags (DG, 2026-11-10; implementation pending).** Actual Bash
option-flags expansions get the ordinary `# MANUAL:` annotation and
pass-through treatment. This is an unsupported bashism, not a new hard
conversion error, an emulation of Bash's flags, or a rewrite to a fabricated
literal value. The equivalent `$-` and `${-}` spellings follow the same rule,
including inside double quotes.

The distinction is syntactic: literal text containing `$-`, including
single-quoted, C-quoted or Bash-escaped text, is not an option-flags expansion.
It must not receive this annotation merely for containing those bytes.
Other conversions and annotations retain their existing contracts.

Native sash keeps its word-level literal-dollar fallback (DD#64);
conversion does not impose Bash parameter meanings on native source.
`q-word-dollar-dash` owns the converter work, including preventing `${-}`
from being mistaken for a `${NAME-default}` expansion. Journal 608 records
the decision and a current converter-only probe. No converter change ships
with this policy recording.

### Best-effort bless — emit `bless` line or `# BLESS:` hint

<del>

| Pattern | Action |
|---|---|
| `x=$(date +%F)` and similar known externals | Emit `bless x [[ $x =~ pattern ]]` with the known pattern from the externals list |
| `x=$(somecommand)` where command is not in known list | Emit `# BLESS: x is assigned from 'somecommand' — add a bless with appropriate pattern` |
| `read x` | Emit `# BLESS: x is read from stdin — add a bless with appropriate pattern` |

</del>

DD93 renames the operation to `attest`; DD99 specifies current reader
conversion and `# ATTEST:` guidance.

## Acceptance criteria

1. `sash-convert` produces valid sash-native source from bash input
2. All mechanical conversions produce correct sash-native syntax
3. Ambiguous cases are annotated with `# CHECK:` hints, including SC2086 and conditional-test captures (the latter remains queued above)
4. Unconvertible constructs are annotated with `# MANUAL:` and left unchanged
5. Known externals get real ~~`bless`~~ attest lines; unknown externals get ~~`# BLESS:`~~ `# ATTEST:` hints
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
