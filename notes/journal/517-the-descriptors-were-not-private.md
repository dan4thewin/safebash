# 517 - The descriptors were not private

2026-10-17. DG declined further temporary-name/UCN work: "we won't pursue
this. continue with coverage".

Started at `013cd89`, with current coverage provenance and 234 compiler
production cold lines. Selected all twelve cold lines in native
`codegen/dispatch.rs`: the legacy test answer, malformed internal operands,
retag fallbacks and bare exec. The source probes exposed real faults before
the API-only contracts were classified.

## A quoted test was silently true

The real `[[ ... ]]` form becomes `SsaInst::Test`. A quoted `'[['` head
bypassed that lowering: native answered true, while C panicked. Applied the
existing quoted-head policy rather than adding another spelling of an
intrinsic. Redundant quoting of compiler-owned intrinsic heads is SB5096;
empty constants retain SB4074 and dynamic heads retain SB5011.

ANSI-C heads exposed a provenance problem: the parser has already decoded
`$'...'` into a Literal, so walking AST quote nodes cannot detect the quoting.
Normalization now receives the exact consumed source spelling. The scanner's
last consumed end excludes lookahead and following quoted comments. Necessary
whole single/ANSI-C quoted paths remain accepted; concatenated or redundant
quoting does not become a second command grammar.

Twelve dual-mode source specs cover the quoted intrinsic families, ANSI-C
spellings, necessary quoting and comment boundaries. The initial before
control failed eight cases in each mode.

## The SSA protocol is not optional

Both backends now share exact-arity, integer-literal and variable-operand
decoding for the transform's internal command protocol. Missing operands,
extra operands, malformed signal numbers and wrong variable shapes no longer
silently skip an instruction or substitute zero. Native retag uses the exact
source version's canonical slot; it cannot quietly drop a retag or borrow a
different version's slot.

The native tests feed malformed SSA to the existing `compile_ssa`; C tests
pin routing through the shared contracts. These are backend API tests, not a
new source harness bypassing validation.

The legacy Test guard passed the rival test by execution. Temporarily
restoring its old always-true answer made its direct-SSA unit fail because
malformed SSA compiled, while the full **8697-check CLI sweep** still passed
apart from the 13 existing C skips. Restored the guard before the final
measurements.

Independent CLI-only attribution used all 32 LLVM merge-pool slots for the
compiler's signature: **8532 profiles**, excluding the unit binary. Native's
unlowered Test guard and the shared own-instruction refusal are zero in CLI
profiles and execute **1 / 2** times in the union. Exact-arity, nonliteral
integer, malformed integer and nonvariable guards are likewise zero in CLI
profiles and **22 / 3 / 8 / 6** in the union.

## Exec was losing its setup result and its backups

Bare/fd-only native exec did not define its condition verdict. More seriously,
native discarded redirect setup failure and could take the wrong branch or
replace the process despite a failed open.

Exec now uses the canonical setup guard. Its successful callback takes
ownership of the backups, discards them, then commits fd-only changes or
replaces the process. Ordinary scoped commands restore instead. C uses the
same ownership order: dispose of exec's backups before its possibly
non-returning body. Bare exec does not rebind diagnostic scope.

Repeated persistent redirects leaked a backup per iteration in both backends;
200 iterations exhausted soft fd limit 128. Restoration also failed to close
an originally closed descriptor again. Shared runtime save/restore primitives
now distinguish that case and make an actual restore failure loud.

Dups used to ignore failure too. Every open or dup now contributes a setup
failure triple, and the first failure prevents later redirect operations.
`sash_redirect` also returns a failed dup2 after a successful open instead of
reporting success. Low-limit probes pin that distinction.

## Private descriptors were visible to source

Raw backup dup copies could occupy an explicit target or make a closed dup
source appear open. They also leaked into external programs. Excluding just
the current redirect list was insufficient: a function closing fds 3-8 could
destroy its enclosing scope's backup, `close 100` could close a private scope
snapshot, and `- close 102; false` could silence the invoker's diagnostic.

DD#14 permits literal nonnegative descriptors; no numeric partition is
reserved for the compiler. The implementation therefore preserves that surface:
the canonical AST walker collects exact fd operands through functions,
captures and END. Both backends register the sorted set before source code
runs. Runtime-owned snapshots, backups, pipes and capture memfds avoid it.
Unrelated numeric data is not reserved.

Allocation scans free descriptors from 3, skipping protected numbers rather
than requiring space above the highest source fd. This preserves free low
slots under tight limits and lets an invalid high fd fail the actual redirect
instead of failing unrelated backup allocation. Scope snapshots retain their
existing minimum of 100 and their established resource-limit fallback.

Private copies and pipe endpoints are close-on-exec, while authored opened
fds remain inheritable. Owned endpoints also avoid stdio numbers even if
those are closed. A lastpipe regression checks that an external invoked from
the reader loop inherits authored fds 3/4 but not the private pipe reader.
Dup/close literals must fit the signed OS descriptor type; overflow no longer
falls through to another fd or wraps.

The guide documents ownership and isolation. DD#14's operand description now
makes the existing OS representability constraint explicit.

## A full sweep caught double restoration

The first expanded sweep ran **8675** checks and found two old-ID regressions,
`native:56.84` and `c:56.84`. A failed redirected function inside END restored
its backups on the failure path, then normal RedirectPop restored them again.
The previous raw dup2 hid the invalid second restoration.

Only an actual abort now consumes backups early. END reports the setup failure
and continues through its normal pop, which owns restoration. The final
baseline comparison proves both regressions repaired.

The fresh coverage pass also exposed obsolete "nothing can fail" scope
fallbacks. All redirect kinds now report failure, so the per-redirect
`Option` relay and its silent defaults were removed. Empty lists remain a
valid collector API input, but not a valid RedirectPush: its sole production
constructor brackets a redirected function call; the parser only creates
nonempty wrappers and spilling preserves list length. Both backends now reject
a lost scope list loudly (DD#116 property 1), pinned by two malformed-SSA
tests. C's empty collector return is zero in CLI profiles and one in the union.

The staged complexity gate caught C's touched `emit_main` at cc4/cog5.
Filtering main blocks before iteration removes the nested continue and the
unnecessary key-vector allocation, preserving block order without a bypass.

## Accounting and measurement

Added **49 dual-mode specs**: errors 417 -> 429 and exec 33 -> 70.
Historical high-water marks were qhead.21, ex.22 and cl.11; new IDs are
qhead.22-.33, ex.23-.57 and cl.12-.13. Both source collision checks are clean.
The only plan/token discrepancy is coverage.sh's established three-ID loop.

Predicted **8697 = 8599 + 2*49**, and obtained exactly that: native **4451**,
C **4246**, aggregate **8684 passing / 13 existing C skips**. Against saved
prechange row boundary **5776093**, all **8599** old IDs match and exactly
**98** IDs are new. Reruns introduce zero new IDs and zero flips; comparison
liveness is **260**. No unexpected retirements.

Before controls have a runtime provenance limit: a saved compiler still links
the current runtime archive. The initial quoted-head and exec controls were
recorded before the later runtime changes; the initial exec batch failed
18 native / 12 C cases. Those are not before-results for the final 70-case
file, and rerunning the saved compiler with today's archive would not be a
pristine whole-project baseline.

Final serialized union, database load and provenance check: native dispatch
**12 -> 0**, compiler production cold lines **234 -> 204**. Parser, lexer,
expression, arithmetic and native-value cohorts remain zero. The filtered
production database records **29634 lines, 204 cold**; raw report headline
**98.94% line / 99.43% function / 93.65% branch**.

Restored ordinary release tools with `cov-reset` and checked the no-profile
canary. Final ordinary units **157 sash + 27 convert + 20 expand**, full
ordinary sweep **8697**, and formatting/clippy/totality/staged complexity
gates pass. No namespace/UCN redesign, new dependency, or confirmed bug
deferred.
