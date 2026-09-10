---
title: "SSA taint defense-in-depth: cmd_subst_taint should propagate operand taint"
status: done
priority: P2
kind: hardening
depends: []
gates:
  - "`cmd_subst_taint` (ssa_taint.rs) propagates operand taint for the pass-through builtins (echo/join/keys/dtoi/itod) instead of hardcoding Clean by command name — every other transform value (Concat/RegexReplace/CaseConvert/Ternary/Substring/DefaultValue/ArithBinOp) already propagates via any_tainted"
  - "join/keys with a bare array-NAME literal operand look up taint_of_name (the literal doesn't carry the array's value-taint)"
  - "len stays Clean (cardinality only, DD#24-exempt); generators (epochseconds/random/argc/getpid/…/getcwd/status/signum) stay Clean"
  - "check_inst_uses extended to cover Test operands and Redirect targets (currently only Command/Pipeline/SubscriptAssign/ArrayAppend args), so the SSA backstop matches the CST positions"
  - "0 spec regressions native/c/janet"
---

Context: the CST taint pass (validate.rs) is the authoritative, user-facing
gate and was made position-uniform (journal 185): a `$( )` that reads
already-tainted data is a use-site in EVERY value position (arg, assignment
RHS, subscript, array-literal, `[[ ]]` operand, redirect target, nested),
plus bare-name `join a`, plus direct tainted vars in test/redirect. See
DD#24 amendment (2026-08-26) and spec taint.sh 60.37–60.46.

The SSA taint pass (ssa_taint.rs) is documented as defense-in-depth, but its
`cmd_subst_taint` classifies join/keys/echo as unconditionally Clean by
command name — the lone transform value that DISCARDS operand taint. This
cannot fire on current programs (they've all been CST-cleared), so it is not
a live leak; but it leaves the backstop unable to catch a future CST
regression in this class (exactly the bug that was just fixed). DG framed the
correct model: every `$( )` is internally `_tmp=$(…)` and should reuse the
normal taint machinery — including the SSA def-classification propagating
operand taint, like every other transform value already does.

Note: SSA taint failures surface as ICE-style errors (not clean SB5020), so
this is a backstop only; the CST pass must remain the diagnostic authority.

Watch-item: `join a` (bare name) needs taint_of_name lookup since the operand
is a Literal, not a value that carries the array's taint. `bless` predicates
legitimately read tainted data — ensure the propagation doesn't regress them
(check_inst_uses already exempts bless args; verify Test-in-bless-predicate).

## Closed 2026-09-10 (backlog prune)

Marked superseded with no successor named. Its subject — the SSA backstop
being incomplete — is covered by the live `ssa-taint-full-backstop`, which
tracks making the backstop a complete gate (capture path, arrays, positions).
Closing as a duplicate of that.
