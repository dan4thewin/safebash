# Design Decision 10: `bash -c` and Bourne-lineage `-c` handling

**Date:** 2026-04-12

**Context:** The adversarial audit (B2) flagged `bash -c "$payload"` as
a CRITICAL bypass equivalent to eval. Initial remediation plan proposed
rejecting all `bash -c` invocations (SB4005). DG challenged this:
nothing stops evasion via a script with a shebang, and safebash doesn't
police external commands like `rm` or `mount`.

**Decision:** Don't reject. Don't ignore. **Compile it.**

- `bash -c 'literal string'` → parse the string at compile time,
  validate against the safe subset, compile as an inline subshell.
  **Allow.**
- `bash -c "$var"` → string not known at compile time. **Reject**
  (same rationale as eval/SB4001: the compiler can't compile code
  it can't see).
- `bash -c "literal with $expansion"` → reject unless all expansions
  resolve to compile-time constants (SSA phase could prove this).

Applies to Bourne-lineage shells: `sh`, `bash`, `dash`, `ksh`, `zsh`
with `-c`. Non-Bourne interpreters (`python3 -c`, `perl -e`) remain
ordinary external commands — not the compiler's jurisdiction.

**Rationale:** This follows the same principle as `source`/`.`
(decision #1). The compiler doesn't reject the construct — it compiles
it away into something analyzable. The dangerous case (runtime string)
is rejected because it's literally uncompilable, not as a policy choice.

The process isolation that `bash -c` implies (clean environment, no
parent locals/functions) maps to the same subshell elision analysis
from experiment 018 — SSA versioning provides the isolation.

**Implication for B2:** SB4005 changes from "reject bash -c" to
"reject bash -c with non-literal argument; compile literal argument
as inline subshell." This is a refinement, not a reversal.

**Revisit if:** compile-time parsing of the `-c` string proves too
costly or introduces unacceptable complexity in the lowering pipeline.

---

## Design Decision 11: Alias handling

**Date:** 2026-04-12

**Context:** Adversarial audit (B5) found that `alias e=eval; e` could
bypass the validator. Discussion explored whether aliases are "just
syntactic sugar for functions."

**Analysis:** Most aliases (`alias ll='ls -la'`) are trivially
convertible to functions (`ll() { ls -la "$@"; }`). The dangerous
aliases are syntax-fragment macros (`alias begin='{'`) that can't be
functions because they inject partial syntax — and those are exactly
the ones we can't analyze.

**Decision:**
- **AOT strict:** Reject `alias` and `unalias` entirely. Error message
  includes the converted function form so the author knows exactly what
  to write instead.
- **JIT coerce:** Convert simple aliases to functions automatically.
  Parse the alias expansion text as a command; if it's a valid complete
  command, emit `name() { expansion "$@"; }`. If it's a syntax fragment,
  reject even in JIT — there's no safe conversion. Emit warning either
  way.

**Rationale:** Every practical alias has a function equivalent. Functions
are parseable, analyzable, and have proper scope. Aliases are text
macros that operate before parsing — the compiler can't reason about
them. Rejecting aliases in AOT loses nothing; converting in JIT
preserves daily-driver usability.

**Codes:** SB4008 (alias rejected / converted).

---

