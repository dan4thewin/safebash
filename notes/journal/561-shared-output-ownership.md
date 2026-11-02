# 561 - Shared-output ownership

2026-11-02. DG retained validation inside a shared producer as an additional
rule, separate from W2a's read boundary. I implemented that choice as W2b:
shared-output provenance and SB5100, using the existing independent
per-function summaries. W3-W5 remain separate.

## Two questions, not two taint rules

The read boundary answers whether data is tainted. Ownership answers where
that tainted value may be attested. Every kernel read still introduces taint;
no pathname, builtin writer or previously attested producer exempts it.

A shared function can internally capture, attest and echo, then export
through an actual compiler buffer. A later real pipe read remains tainted
and cannot be attested outside that shared producer. With the existing
transport paths, even a shared builtin-only pipe writer can therefore
require `:!` at its receiver. Capturing or retaining its tainted output is
not itself forbidden; explicit trust at the use remains legal.

DG's suggestion of buffer transfers from completely known sash-generated
writers is recorded in `q-buffer-capture-eligibility`. It must replace the
read while preserving DD122's producer isolation, streaming, early-close
and status semantics. This unit does not widen buffer eligibility.

## Origins alongside the existing name state

`Tainted` retains W1's tainted-name set and adds possible origins. Read-only
checks still consume the name set; mutation methods maintain both parts.
Branch, return and self-tail joins preserve origins as well as names.
Ordinary-loop convergence now compares both; its existing policy and cap
remain W3's work.

The summary contains stdout paths, origin-bearing generators, true origin
flows and deferred attest obligations. A shared function takes ownership
only of still-direct stdout paths. An already-owned inner path keeps its
innermost shared producer, so a shared wrapper cannot hide it. Structural
bodies and in-place integer calls contribute output; an ordinary nested
capture has its own output boundary. Capture ownership is assigned only
after the unchanged buffer/read classifier says the result is tainted.

**Boolean flow is not provenance flow.** W1 deliberately retains the input
identity when a fresh source re-taints a name. For example, a function
unconditionally replacing global `x` with a new external capture still
returns a tainted `x`, but none of the old value's owners should follow it.
Singleton summary probes now carry an explicit incoming marker; only exits
retaining that marker forward the caller's origins. The boolean transfer
itself is unchanged.

Whole assignments and ordinary readers replace old origins; `+=` retains
them. Value splits and whole collection unsets also discard the replaced
value's provenance, without claiming to fix W1's separate conservative
boolean treatment. Partial unsets do not erase the remaining collection's
origins.

At an attest, an owned subject raises SB5100 with the shared producer and
its call sites. Arithmetic conversion checks the source subject, not the
conversion target's old value. An incoming-name or stdin marker instead
becomes a summary obligation. Calls substitute actual origins and relay
the diagnostic, including its body location and call chain. Globals, slots,
forwarding and permutations use the existing mapping and non-aliasing
contract; no caller-specific callee walk was introduced.

## Stdin is lexical too

A parent reader's private pipe is not ordinary stdin. Calls and anonymous
captures in its body use the enclosing stdin; forked stages receive the
preceding output. Function summaries start with symbolic stdin and export
its reader/capture obligations, including through helper calls and slot
generators. Arbitrary external commands may consume that stdin; there is
no executable-name whitelist.

A redirected statement suspends the private reader binding. An explicit
stdin open is a new read source, still tainted; descriptor dups retain
inherited-input obligations. This tracks syntactic source ownership, not
physical descriptor aliasing or isolation. It does not revive the discarded
pathname-based clean-read analysis.

## Specs, draft defects and the existing fixtures

`outown.1`-`.34` bring taint.sh from 483 to 517. They cover shared and nested
owners, multiple paths, scalar/array/loop readers, caught and converting
attests, global/slot summaries, returns, in-place value calls, stdin and
replacement boundaries.

The final before-control used the immutable W2a compiler. All 34 programs
compiled and ran with exit zero and no compiler/runtime diagnostics;
exactly 25 assertions failed per mode because SB5100 did not yet exist.
The nine preservation controls were `.16`-`.19`, `.25`, `.26`, `.28`,
`.33` and `.34`. Earlier drafts included invalid grammar, an unused
collection and a SIGPIPE-prone reader; those were corrected before accepting
the control. The authoritative files are `before-input-final-*.tsv`,
not the earlier draft logs. These results establish the new rejections,
not equivalence of all previously accepted runtime outputs.

The first ownership draft falsely rejected `.33` and `.34`: value splits
and whole unsets retained the incoming-origin marker. Clearing that metadata
fixed both. The existing attest-related cohort then exposed two introduced
ICEs, `30.44`/`30.45`, in both modes. The new unset handling had called
lowering's literal-only decoder before SB5063 rejects quoted/sigilled binding
operands. A literal guard leaves those malformed operands to the existing
diagnostic; both original specs pass unchanged.

Only one existing fixture needed a policy adaptation: `pipeown.4` uses its
shared builtin writer through three real pipes. Its caller attests are now
SB5100, so its final echo uses explicit trust instead. All three nested
readers and the exact eight-line output assertion remain.

The other 45 file/mode runs in the attest cohort had no failures beyond
those two unset cases and `pipeown.4`. After the fixes, taint.sh, types.sh
and pipe_functions.sh pass in both modes. All runs were serial and used
the repository compiler/helper paths explicitly.

## Independent reversion proofs

Eight separately built reversions pin every preservation control. Each
produced the same red set in native and C, and no old taint.sh failures:

| reversion | red `outown` ids per mode |
|---|---|
| forward origins through W1's boolean flows | 16, 33, 34 |
| include nested capture bodies in outer stdout paths | 19 |
| let captures inherit the private reader pipe | 26 |
| relay call stdin obligations from the private reader pipe | 26 |
| remove the name but retain its origins on replacement | 16, 17, 28 |
| stop recognizing explicit stdin opens | 25 |
| stop clearing value-split/whole-unset origins | 33, 34 |
| reject owned captures at assignment instead of attest | 16, 17, 18, 19, 28 |

The actual xtraces contain SB5100 for all 34 failed preservation assertions
across these sixteen runs, not an incidental compiler/runtime failure.
Every stub was restored. Both source hashes and the rebuilt compiler match
the saved positive byte-for-byte, before and after the full gate.
Standalone controls wrote only their owned TSVs; spec.db remained at row
6680958 until the gate.

## Gate and records

`NEW=68 FLIPS=0 RETIRED=0 TOTAL=10782 mk -p 1 gate` passes first run.
The sweep took 241 seconds, rows **6680959-6691740**: native 5502, C 5280,
**10769 passing plus 13 skipped**. Exactly 68 first-ever ids, zero flips,
retirements, ghosts or plan mismatches; liveness is 260. Units are
237/27/20. The real formatting, clippy-with-tests, totality and staged live
complexity gates pass without bypass.

DD127 is rewritten in place; GUIDE, the findings index and the buffer
backlog agree. H327 records plan 483->517; ledger statuses are unchanged.
H282's W3 part and H327/H329's W5 parts remain pending. No runtime, lowering,
SSA, indexnudge, dependency or SOP change was needed.

Evidence is in session `files/port-w2b-probes.tar.gz`: **232 verified
members**, including 47 complete xtraces, before/after and reversion TSVs,
the exact gate row block, implementation patch and source snapshots.
Compiler binaries are excluded; their hashes are archived and the frozen
W2a/W2b binaries remain in `/tmp/safebash-w2.gFBe6Co0`.
The earlier ownership-decision probe archive remains separate.

W2 is complete. Ordinary loop edges, per-process SSA, END entry and the
already-recorded precision gaps are not claimed here. No coverage run or
reference-branch change was made.
