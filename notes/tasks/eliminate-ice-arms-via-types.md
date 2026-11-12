title: Eliminate ICE panic arms by pushing invariants into the type system
status: done
depends: []
gates:
  - per-site decision recorded (refactor / typestate / keep+cite)
  - each refactor lands in its own commit with all gates green
  - remaining sites carry a comment naming the upstream pass that
    establishes the invariant

# Eliminate ICE panic arms by pushing invariants into the type system

A `panic!("ICE: …")` arm is an admission that the type at that
point admits a variant the type system can't prove impossible.
In a project with zero-tolerance-for-bugs discipline, the runtime
check duplicates the bug discipline — compile-time would be
strictly better.

This task is the successor to
[codegen-panics-to-diagnostics](codegen-panics-to-diagnostics.md)
(done — converted user-facing failures to diagnostics). The
remaining sites are *internal* invariants, not user failures, and
the right move is to make them unrepresentable rather than
catchable.

## Snapshot (2026-06-11)

23 sites in non-test `sash/src/`. Strongly clustered in codegen
(20 of 23).

```bash
sqlite3 .indexnudge-rust.db "
SELECT file, line_num, substr(trim(content), 1, 90)
FROM lines
WHERE file LIKE 'sash/src/%' AND file NOT LIKE '%tests.rs'
  AND (content LIKE '%panic!(\"ICE:%' OR content LIKE '%unreachable!(%')
ORDER BY file, line_num;
"
```

| file | sites |
|---|---|
| `sash/src/codegen/value_handlers.rs` | 8 |
| `sash/src/codegen/types.rs` | 5 |
| `sash/src/codegen/values.rs` | 4 |
| `sash/src/codegen/arithmetic.rs` | 2 |
| `sash/src/emit_c.rs` | 2 |
| `sash/src/codegen/inst_handlers.rs` | 1 |
| `sash/src/transform/expr_handlers.rs` | 1 |

## Dominant patterns

Three patterns account for the bulk of the sites:

1. **VarSlot Int/Str mismatch** (4× in `codegen/types.rs:23,29,35,41`)
   — `panic!("ICE: expected string var, got int")` and the dual.
   `VarSlot` is a single enum lumping both kinds; every consumer
   that expects one kind has to panic on the other. Candidate:
   split into `IntSlot`/`StrSlot` with the symbol table parameterized
   on caller expectation, or typestate the lookup.

2. **"missing literal" lookups** (6× in `value_handlers.rs:130,136,
   173,180,517,811` and `values.rs:576,654`) — string-keyed lookups
   into an interner that the code "knows" is populated. Candidate:
   typed interner key (`LiteralId` instead of `String`) so the
   lookup is total by construction.

3. **"undefined var/array" lookups** (3× in `inst_handlers.rs:836`,
   `types.rs:228`, `values.rs:736,802`) — symbol-table lookups
   that the code "knows" earlier resolution populated. Candidate:
   typestate the symbol table after resolution
   (`SymbolTable<Resolved>`), or hand back a `SymbolId` from the
   resolution pass that is total in the consumer's table.

The remainder:

- `codegen/arithmetic.rs:34` `ArithOp::Not in binary context` —
  split `ArithOp` into `UnaryArithOp` and `BinaryArithOp`.
- `codegen/arithmetic.rs:43` `comparison ops handled by fast path`
  — refactor so the comparison dispatcher takes a non-comparison op.
- `transform/expr_handlers.rs:144` `join requires at least one arg`
  — non-empty-vec type (e.g., `Vec1<T>`) for the argument list.
- `value_handlers.rs:758`, `emit_c.rs:964,1328` `_ => unreachable!()`
  — bare arms with no message; first triage step is to add an ICE
  message so we can tell what invariant each one is asserting, then
  apply the refactor.

## Per-site decision protocol

For each site, **decide one of three** (and record the decision
in the commit message and/or a code comment):

1. **Refactor — make unrepresentable.** Preferred when the invariant
   is local (one type, one module). Costs: API rippling, one PR.
   Wins: arm disappears, cyclomatic drops by 1, future maintainer
   can't hit it.

2. **Typestate — pipeline boundary.** Preferred when the invariant
   crosses pass boundaries (e.g., "transform populates slots before
   codegen reads them"). Cost: heavier refactor across pass
   boundaries, but retires a *group* of related arms at once.
   Candidate states for sash:
   `Ir<Raw> → Ir<Validated> → Ir<Typed> → Ir<Resolved> → Ssa<Built> → …`
   Discuss scope with DG before starting.

3. **Keep — cite the upstream invariant.** Acceptable only when
   the invariant genuinely requires dependent types to express, or
   the typestate refactor would cost more than it pays back.
   Minimum requirement: a `// INVARIANT: …` comment naming the
   upstream pass that establishes the invariant AND the code change
   that would invalidate it. `grep`-findable from the upstream pass.

## Sequencing

Sub-tasks below are the natural cohorts. **Each cohort needs a DG
go-ahead before starting** (per AGENTS.md "Decision authority"):

- **A. Quick local wins** (`arithmetic.rs`, `expr_handlers.rs:144`,
  the three bare `unreachable!()` triages). ~5 sites. Likely one
  PR each, low risk.
- **B. Literal interner typed keys.** ~8 sites in
  `value_handlers.rs` + `values.rs`. One refactor PR retires the
  whole group if `LiteralId` lands cleanly.
- **C. VarSlot split.** ~4 sites in `types.rs`. Decide between
  enum-split vs typestate-after-resolution. Larger blast radius —
  touches every codegen consumer.
- **D. Symbol-table-after-resolution typestate.** Overlaps with C
  if we go the typestate route. Discuss scope with DG.

## Adjacent cleanup (not part of this task)

`sash/src/oracle.rs` is test scaffolding (runs `Command::new("janet")`
and `Command::new("bash")`) used only by `oracle_tests.rs`. It
contributes 49 LOC at 0% spec coverage and 0% production usage
because it's never reached from a compiler invocation. Move to
a test-only module or to `tests/common/`. Tracked separately if
filed.

## References

- [codegen-panics-to-diagnostics](codegen-panics-to-diagnostics.md)
  — done predecessor (user-facing panics → diagnostics)
- [cohort-complexity-cog-cc](cohort-complexity-cog-cc.md) — each
  ICE arm is +1 cyclomatic on its host function; refactoring out
  compounds with the standing cohort work
- AGENTS.md "Bug policy" — zero tolerance; ICE fire is a
  stop-everything event, which is exactly what compile-time would
  prevent

## Re-measured 2026-09-12

**16 sites**, down from the 23 in the 2026-06-11 snapshot. The cohort A–D
structure and the per-site decision protocol still apply; the file
distribution should be re-derived before starting, since seven sites have
gone.

## Resolved 2026-10-05 — commit `50ab2f2`

DG supplied the selection criterion this task was missing: an unreachable
line earns its place only if it catches a regression **orthogonally**. One
that re-asks the gate's own question about the same data is a second copy of
one assertion.

That is decidable from the ICE's own parameter type (see AGENTS.md,
"An ICE must catch a regression ORTHOGONALLY"). Measured over all **107** ICE
sites: only 20 name an SB rule, of which **11 were duplicates**.

Per-site decisions, as the gates require:

| site | rule | decision |
|---|---|---|
| `control_flow.rs` ×4 (`detect_array_iter`) | SB5009 ×3, SB4045 | **refactor** — shape spelled out once as `ir::for_head_keys`, read with `?` |
| `expr_handlers.rs` (`try_len_intrinsic`) | SB5062 | **delete** — see note below |
| `expr_handlers.rs` (`join_intrinsic`) | SB5062 | **delete** — same |
| `stmt_handlers.rs` ×2 (`handle_argv`) | SB4045 | **keep+cite** — `try_builtin`'s `None` = "not a builtin" → silent runtime death |
| `stmt_handlers.rs` (`handle_keys`) | SB5073/SB4045 | **keep+cite** — same |
| `ir.rs` ×2 (`readline_target`) | SB5057 | **keep+cite** — declining emits no `ReadLine`; the program reads nothing, silently |

The nine orthogonal sites are untouched and correct by the criterion; two of
them carry comments recording bugs they actually caught.

**The second criterion, which this task did not anticipate:** *what does
declining cost?* A duplicate is only removable when the fallback is a safe
decline. Where `None` means "not a builtin" it is a measured catastrophe, and
the panic is the only way to say "malformed". Cite the consequence, not the
gate name.

**Remaining:** the 87 structural ICE sites (exhaustiveness pins, totality
markers, same-function-already-handled arms) were not in scope here and are
governed by "Cold arms: ICE or answer?" in AGENTS.md.

Status: **done** for the SB-citing cohort.


### Amendment 2026-10-06 — `6a44932`

The two SB5062 sites were first given the *correct answer* (`pe.name`) rather
than deleted. DG rejected that: **"delete - it risks a reader mistaking the
spelling as legitimate"**. Removing the duplicate assertion left the arm
unreachable, and an unreachable arm that handles a rejected spelling documents
that spelling as supported.

Verified by execution first, since the SB5062 claim had been taken from a doc
comment: every splat spelling raises SB5062 at the single site that raises it
(`validate.rs:8833`) — `len @a`, `join @a ,`, `len @args`, `join @args ,`,
`len @{a:-}`, `len @{a:~}`, `"$(len @a)"`, `join $a ,`; `len $a` is SB5064; the
bare forms compile.

So this task has **four** outcomes, not three, and they are ordered:

1. **refactor** — share the predicate so the question is asked once;
2. **delete** — the fallback is a safe decline, or the arm is unreachable;
3. **answer** — only where the arm is, or becomes, *reachable*;
4. **keep+cite** — declining is a measured catastrophe; cite the consequence,
   not the gate name.
