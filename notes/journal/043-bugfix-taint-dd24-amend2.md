# Bug fix: taint analyzer skipped function bodies (2026-04-24)

**Discovered during a TODO.md gap survey.** While steelmanning YAGNI on
"taint inheritance through function calls," DG asked the obvious
verification question: *if `getenv` is called inside a function body,
is the result tainted?*

Empirical answer: **no, it was not.** The analyzer compiled this without
diagnostic:

```bash
foo() {
    local x
    x=$(getenv USER_INPUT)
    echo "$x"
    cmd "$x"
}
foo
```

This is a validator bypass — exactly the class of finding the overdue
adversarial-audit run-2 is meant to catch. We found it without running
an audit because DG asked the right question.

### Root cause

`walk_statement` matched `function_definition` and called
`handle_function_def`, which only recorded the function name and never
descended into the body. The four DD#24 taint sources
(`getenv`/`argv`/`read`/`$(external)`) were therefore unanalyzed
anywhere inside any function body.

A misleading comment I had added the same day asserted that "parameters
are clean by construction" — true for the call-site-rejection of
tainted *arguments*, but it concealed the real gap, which was that the
body wasn't walked at all. New taint *created inside* the body was
invisible.

### Fix

`handle_function_def` now walks the body with `walk_statement`, after
seeding clean status for the function-local positionals (`$1..$9`,
`$@`, `$*`). Variable state is snapshotted before the walk and restored
after, so taint from inside the body does not leak to siblings.

This is intra-procedural analysis only. No signatures. No propagation
across call boundaries. DD#24's quarantine model is preserved: external
data is rejected at the boundary it crosses, and a sash function call
site is itself a boundary (handled by the existing argument check in
`handle_command`).

### Why function-local positionals are clean

Inside `foo()`, `$1` refers to `foo`'s first call-site argument, not
the program's argv. The call site is a boundary: `handle_command`
rejects any tainted argument expression at the call. Therefore inside
the body, `$1..$N`/`$@`/`$*` are clean by construction. (This is the
correct version of the claim the prior comment got wrong.)

### Tests

Added `neg_tainted_env_inside_function_body` to `e2e_tests.rs`. It
fails before the fix and passes after. Existing function-argument
tests (`pos_func_args_arithmetic`, etc.) continue to pass because the
positional-clean seeding matches the call-site-quarantine semantics.

Full suite: 458 pass, 1 ignored (was 457 + 1).

### Lessons

1. **DG's verification question caught a bug my steelman assumed away.**
   I argued "the call-site check is airtight" without testing whether
   the call-site check was the only check that mattered. It wasn't.
   The body-walk was missing entirely.
2. **A confidently wrong comment is worse than no comment.** I had just
   written "parameters are clean by construction" as DD#24
   reinforcement. It locked in the wrong story. Reverted.
3. **Bug policy worked.** Stopped, fixed, didn't route around. The
   feature TODO entry now reads "incidentally enforced by the
   call-site check" — true *and* sufficient, because the body is now
   walked.

## Design Decision #24, amendment 2 (2026-04-24): no propagation, only classification

The original DD#24 framed taint as flow-sensitive: "reassignment
re-taints," tainted values flowing through assignments until blessed.
Working through the SSA-replatform spec exposed that this framing was
wrong — and the wrongness explains every bug we've found in `taint.rs`.

### The corrected rule

A value is tainted iff its **defining instruction** is one of the
boundary sources. There is no propagation. Tainted values cannot be
copied (`b=$a` is a read of `$a` and is rejected), concatenated,
indexed with, or used in arithmetic. The only legal operation on a
tainted value is to validate it via `bless`.

### The complete source list

- `$(getenv VAR)`
- `$(argv N)` / `$1..$9` / `$@` / `$*`
- `read` / `mapfile` from any inherited or opened fd, or any pipe
  (heredoc / here-string-of-clean produce clean values)
- `$(cmd)` for external `cmd`

User-defined sash functions cannot introduce taint. A function whose
body touches a tainted source without blessing fails to compile, so
`$(user_func ...)` capture is always clean (or compilation has
already failed). No signatures, no inference.

### What "read" means

Every non-bless use is a read: command args, RHS of assignment,
arithmetic, subscript indices, `join` over an array containing any
tainted element, function-argument passing. All rejected.

### Containers

Arrays/maps are containers of independent variables. Each element
carries its own taint. `join @arr` is rejected if any element is
tainted, exactly as if the elements were separate scalars.

### Why this is simpler

- No flow analysis. Per-definition classification + per-use rejection.
- No "taint propagation through assignment" — copying is reading.
- No inter-procedural reasoning — functions can't introduce taint, so
  call boundaries are not interesting.
- No scope-shadowing complications — at the SSA layer the question
  "is `x` tainted?" is "is the SSA defining instruction of this
  version of `x` a source instruction?" Yes/no, no map state.

### Why CST-layer enforcement keeps breaking

The CST analyzer has been trying to model "flow-sensitive taint with
reassignment re-taint" — the original (wrong) DD#24 framing. That
required tracking per-name state across statements, modeling scope,
walking function bodies, handling locals. Every layer of that was a
bug surface. The corrected rule needs none of it: just look at each
definition and classify, then reject reads.

### Implementation plan

Move taint enforcement to a pass over SSA. Per `SsaInst::Assign`,
classify the target's taint by the defining `SsaValue` shape (source
patterns → tainted, everything else → clean). Per use site
(`Command`, `Pipeline`, RHS of any other assign), if any operand is a
tainted `VarRef`, emit SB4020 unless the use is the test expression
of a `bless`. `bless` produces a new clean SSA version of its target.

The CST analyzer keeps only purely-syntactic checks: bless argument
shape (must be a name + test), heredoc-with-cmdsub rejection (SB5002),
eval rejection (SB4001), indirect expansion rejection.

### Lesson

DG's instinct to ask "is `getenv` inside a function tainted?" was the
right question. The answer exposed not one bug but a wrong mental
model that had been embedded in `taint.rs` from the start.
"Flow-sensitive with reassignment re-taint" sounded conservative and
felt right. It wasn't conservative — it was an attempt to mimic
Perl's taint mode in a CST analyzer, which is exactly what DD#24's
prose said we wouldn't do. The amendment makes the prose match the
intended semantics.

