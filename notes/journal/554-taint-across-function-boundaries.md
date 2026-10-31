# 554 - Taint across function boundaries, written down once

2026-10-31, after F10 (journals 552-553) and before W1. Notes only: no
code and no spec changes.

DG proposed a refinement to T1. A function whose only call site is a
capture is its body transposed into the `$( )`, so an attest immediately
after the capture satisfies DD121's adjacency rule. That makes the
ergonomic case, a function whose whole body is one command or pipeline,
work without an attest inside it. I recommended option (b): apply the same
transposition to names going outward. A single-use call is then one
compound statement, so its caller may attest what the call leaves in a
global or a slot, immediately after the call. DG: "go with b - do a full
writeup focused on clarity (perhaps in DD100, avoid the ammendment pattern
where later statements contradict earlier ones resulting in confusion).
proceed with the port work".

## The rule

DD127 states it in one sentence: "Own taint is attested in the body that
produced it, or — when that body has exactly one call site — immediately
after that call. Received taint is never attested inside a body."

- A function is single-use if it has exactly one call site. An anonymous
  `$( )` body is single-use by construction. A function with zero, two or
  more call sites is shared.
- **Outward.** A single-use call leaves the caller in state (S − K_f) ∪ G_f.
  The caller attests immediately after the call, or a later read is SB5020.
  A shared function that leaves its own taint behind is SB5067 inside it.
- **Inward.** A body's use of taint it received is rejected at the call,
  with the code the use would get if the taint were the body's own: SB5020
  for a read, SB5021 for a cell write, SB5088 for an attest. `:!` stays
  legal.
- **stdout** (T1, W2). `$(f)` is tainted when unattested external output
  reaches `f`'s stdout. A single-use `f` is attested after its capture, a
  shared one inside its body.

Each body is checked once and summarised, and the summaries are applied at
the calls (DD127 "How it is checked"). A body's verdicts therefore never
depend on its callers.

## Why a new DD

This was decide-and-report. DG suggested DD100, but the model covers three
channels and DD100 owns only one of them, slots. stdout belongs to DD24 and
globals to DD121's SB5067. Writing the model into DD100 would make a
collection-parameter DD the authority for function output and for globals.

So DD127 states the whole model once. Every older text it contradicts is
rewritten in place to agree with it and point at it, rather than amended,
and the old text stays in git at 0149c49b. Where the compiler still applies
the earlier rule, the rewritten text says so ("until W1 lands", "until W2
lands"). DD127's "Implementation status" lists every difference.

## What was rewritten

- **DD121.** A new section, "SB5067 — what a body may leave behind",
  replaces the 2026-09-30 addendum that re-decided SB5067. It covers:
  - DD127's rule;
  - why an in-body attest is enough;
  - why the diagnosis keeps its span;
  - why the code number stays;
  - how each body reports only its own sites;
  - why the SSA backstop is not the authority;
  - a History subsection for the 09-30 net-effect rule and its extension
    to `$( )` bodies.

  The structural positions go from four to three: an attest may no longer
  stand where a slot arrives in a body. The "Why" bullet, the fixes table
  and See also now point at DD127. The 2026-10-02 addendum is unaffected
  and stays.
- **DD100.**
  - Part E's table says what decides a slot's taint, instead of where the
    attest goes.
  - "Taint through a slot" replaces "The rest needs no new machinery".
  - The consequence "diagnostics can point inside a callee because of a
    caller" becomes "a caller's taint is reported at the call".
  - Part F's slot-taint section keeps DG's 2026-09-11 out-parameter
    decision for a single-use function. It describes `analyze_slots`' two
    halves, binding and taint; W1 replaces the taint half.
- **DD24.**
  - The source list gains `$(f)` and `f | read`.
  - "User-defined sash functions are trusted" becomes "not a source in
    itself".
  - The 2026-05-24 pipe clarification gets the same qualification.
- **Elsewhere:**
  - DD63's `$(cmd)` bullet;
  - the AGENTS Taint system list, plus a paragraph with the rule;
  - a GUIDE paragraph after the two-phase description;
  - the findings index: the T1-T3 sentence, B33, B92, B115 and a W1 scope
    line.

Comments that describe today's behaviour are left for the step that
changes it, so that each is rewritten together with its flip:
- the spec comments on collparam 100.28 ("Part E: the attest may be
  deferred INTO the callee", W1), on taint.sh's two SB5067 blocks and
  coverage.sh's cov.264 block (all three describe the net-effect rule, W1),
  and on dd82.4 ("a user-function capture is Clean by DD#24", W2);
- code comments describing the net-effect rule, at validate.rs 6068, 6822,
  9372 and 9403 (W1);
- code comments saying a user-function capture is lowered before SSA taint,
  at ssa_taint.rs 623, 634 and 1059 (W2).

## Three stale statements found while rewriting

- **DD100 Part E's SB5021.** It said that a tainted write inside a function
  gets "the caller's later read … rejected SB5021". SB5021 is DD61's
  rejection of a tainted value written into a collection cell. Only
  `reject_tainted_into_array` raises it, at the write. A later read of a
  tainted collection is SB5020, which 100.30 pins. DD100 now states both.
- **DD121's backstop citation.** The 09-30 addendum cited the readonly
  backstop at `ssa.rs:1492`, which hard-coded `"1:1:"`, as evidence that
  SSA is spanless. F13 (68680d82, journal 532) deleted that code. The SSA
  taint backstop's message is still spanless, for its own reason:
  `ssa_taint.rs` builds it with no span, and `run_ssa_taint_gate` in lib.rs
  prints it as it is. The new text cites that message rather than a line.
- **DD100's Open list.** It still said that a function defined inside
  top-level control flow keeps the old definition-point walk. That task
  (taint-fn-in-toplevel-control-flow) is done, so the bullet is deleted.

## Decide-and-report

- DD127 rather than DD100, as explained above.
- DD127 uses `---` frontmatter, as DD116-DD119 and DD122-DD126 do.
- An inward error is reported at the call, with the code the use would get
  if the taint were the body's own. For an anonymous body the error points
  at the line inside it, because that body is written where it is called.
- Attesting a clean received name stays a legal no-op. The doc comment on
  `check_adjacency` in validate.rs says so: "a no-op, not a bypass". (DD127
  first attributed that sentence to DD121, which does not contain it; the
  fact-check before this commit corrected it.)
- Zero call sites count as shared.
- How call sites are counted:
  - a call inside a loop is one site;
  - a self-tail call is not a site;
  - a tail call to another function is a site of that function.
- **Chains.** Every function between a capture and the command it reaches
  must be single-use. Otherwise the rejection names the innermost shared
  function, which is the one that must hold the attest, and its call
  sites. This is more precise than "the first shared one" in my 17:59
  message to DG.
- **A reversal.** DD121's 2026-09-30 extension of SB5067 to `$( )` bodies
  is replaced. An anonymous body is single-use, so its caller attests after
  it, and bfb.23, bfb.24 and bfb.28 go from SB5067 to accepted at W1. This
  reverses part of a 2026-09-30 ruling made with DG rather than narrowing
  it, so the report flags it.

## Evidence

All five probes ran on 0149c49b, native.

| probe | program | today | under DD127 |
|---|---|---|---|
| w1 | `fn g { /usr/bin/git @args }; g status; g log` | compiles | legal: the output goes to the program's stdout, which is not a capture |
| w1c | two captures of `g`, each attested after | compiles | rejected at the call site, because `g` is shared (W2) |
| w1d | one capture of `g`, used unattested | compiles | SB5020 at the echo (W2) |
| b115 | `fn h { argv A; attest e in A […] }; h =g; for v in @g …` | SB5020 7:10 (B115, a false rejection) | legal (W1) |
| n1 | an attest of `a` inside an `if`, after `argv a` | SB5088 3:20 | SB5088 3:20 |

The derivation of the inward rule rests on n1. A single-use body is a
nested block, not the block of the statement that tainted the name, so an
attest inside it is not adjacent.

## Expected spec consequences

These come from DD127, and W1 and W2 measure each one.

- **Unchanged:**
  - bfb.17, .19, .22, .25, .26 and .27;
  - atslot.7 and .8;
  - 100.29, .31 and .33;
  - 100.30 and .32, which stay SB5020 at the read.
- **bfb.18, .20, .21:** SB5067 becomes SB5020 at the later read, because
  each function has one call site. W1 keeps SB5067's span pinned with a
  function that has a second call site.
- **bfb.23, .24, .28:** SB5067 becomes accepted.
- **100.27:** still SB5020, now reported at the call.
- **100.28:** rc 0 becomes SB5088 at the call. W1 adds a spec in which a
  body fills a slot and attests it, which keeps the statement-form helper
  pinned.
- **cov.264-cov.269 and cov.341:** each pins SB5067 with a function that
  has one call site, so none of them is SB5067 any more. cov.266 reads its
  global afterwards and becomes SB5020 at the read. The others never read
  it again, and W1 measures what they raise. W1 gives each function a
  second call site, so that SB5067's two raise paths, a bulk fill and an
  assignment, stay pinned. cov.270 is unchanged. A census found these
  before the commit: 14 non-comment spec lines name SB5067, and each sits
  on a chk line (bfb ×6, cov.264-270, cov.341). DD127 first listed only
  the bfb ones.
- **A shared function that leaves its own taint behind** becomes SB5067,
  whether it re-taints a global (the glob1 shape) or fills a slot and does
  not attest it. W1 measures which specs have either shape.
- **The 15 specs parked for B33:** W2 decides them.

## Next

W1 (Order item 17), per DG's "proceed with the port work".
