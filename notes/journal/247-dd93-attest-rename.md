title: 247 — DD#93 attest ergonomics: Stage 1 (bless → attest rename)
date: 2026-09-06

# DD#93 attest ergonomics — the design thread + Stage 1

DG opened on bless ergonomics: "assigning an int var rather than using a `-i`
test could perform the same check and save a step, e.g., `bless numstr ((
myint = numstr ))`." The conversation that followed produced a four-part
bundle (DD#93), of which this is Stage 1.

## The design thread (2026-09-05/06)

The pivotal moment was DG rejecting my first framing. I had offered **Option A**
— "make every arithmetic result clean," since DD#77's D1 already forces every
string operand in `(( ))` through an `is_int` gate, so an arithmetic result is
*already* a validated i64. DG: "I'm concerned that option A makes the audit
trail harder to follow." That killed A: the taint system's whole discipline is
that **every declassification is explicit, named, and greppable** (`attest`,
`:!` DD#91, `--no-taint` DD#82). A silent arith declassifier would erase the one
property the system exists to provide.

What DG's original spelling got right — and I'd missed — is that it *keeps the
keyword*. `attest numstr (( myint = numstr ))` still has the greppable anchor;
it just lets the predicate be an int-assignment (which validates by
construction) instead of a redundant `[[ -i $x ]]` + separate `:?`. So it saves
the step **without** erasing the trail. That reframing (a completion of DD#77's
`-i`/disposition split, not a new hole) is the spine of DD#93 Part D.

The thread then accreted three more ergonomics:
- **Part B** — unary shorthand `attest VAR OP` ≡ `attest VAR [[ OP $VAR ]]`
  (DG: "monadic tests like -n and -z could forego the ceremony").
- **Part C** — content-shape tests `i?`/`p?`/`g?` (DG: "equivalent of isprint
  for the whole string, and isgraph … since -X space is saturated, maybe spell
  as p? and g? (and maybe -i should be i?)"). Non-empty, ASCII-only.
- **Part A** — rename `bless` → **`attest`** (DG: "an early reviewer balked at
  'bless' the word, not at taint testing — maybe replace with allow"). We
  landed on `attest` over allow/trust/vouch/untaint: "allow" is action-permit
  vocabulary; "trust" is already taken by the *unchecked* bypasses (`:!`,
  `--no-taint`), so using it for the *checked* primitive would blur that line.
  "attest" = certify a checkable property, with the predicate as evidence, and
  it sits in the reviewer's own vocabulary (SLSA/sigstore/in-toto).

Locked (DG): Part D cleans **both** source + target, **distinct** names,
**scalar** only for now. Part B extends to attest-all (`attest e in a OP`).
Part C `i?`/`p?`/`g?` are **general** operators (usable anywhere `-i` is, not
just in attest). Hard rename, **no** `bless` alias. Full DD in
`notes/decisions/DD93-attest-ergonomics.md`.

## Stage 1 — the rename (this commit)

A hard, whole-tree rename of the live surface. `bless` is no longer a keyword:
`classify_word_token` maps `"attest" => Tok::Attest`, and a stray `bless`
degrades to an ordinary (unknown) command → SB5011 (or a parse error when
followed by `[[`). No alias.

Scope: renamed **code** (`sash/src`, `sash-rt/src` incl. the `sash_attest_fail`
runtime symbol + the `sash/janet/sash-rt.janet` prelude's `sash/attest`
/`sash/attest-checked`, and the `sash-rt/csrc/test_regex.c` comment),
**converter** (`sash-convert/src`), **specs**, **Rust tests**, and
**current-state docs** (README, GUIDE, REVIEW; one stale line in AGENTS.md).
**Left historical** (records of what was decided when it *was* bless):
`notes/journal/*`, `notes/decisions/*` (prior DDs), `docs/TIMELINE.md`,
`audit/*`, and the `run-compiler-tasks.sh` task-name references (they point at
un-renamed `notes/tasks/` filenames — changing them would break the lookup).
DD#93 is the forward anchor for the lineage.

Identifiers renamed too (not just the keyword string): `Tok::Attest`,
`AttestForm`, `parse_attest`, `taint_attest`, `emit_attest*`, `BuiltinCmd::
Attest`, etc. A half-rename (keyword `attest`, code says `bless`) is exactly the
drift that got SUBSET.md deleted; and Stages 2–4 add code referencing these
names, better `attest_*` from the start.

Specs: taint.sh gains DD#93 Part-A anchors — 93.1 (`bless` now SB5011), 93.2
(`attest` clears taint end-to-end; native,c — janet elides validation + lacks
getenv), 93.3 (diagnostic now says "without attest").

### Verification

- native + c: taint.sh 132/132. 3-mode authoritative spec.db diff (latest-run-
  per-test vs `HEAD:spec.db`): **0 regressions** native/c/janet; 9 new 93.x
  rows (native/c all passing, janet 93.1/93.3 passing, 93.2 skipped/gated).
- Rust suites (known benched/rot): the 6 attest-named failures are all
  pre-existing — 5 present in the baseline under their `bless_*` names, and
  `pos_attest_numeric_check` fails on SB4030 (DD#77 D1's bare-string-in-arith,
  orthogonal to the rename). e2e (47) and taint_spec (25) failing counts match
  the documented baselines exactly. No attest/bless-named failure in
  codegen/c/janet/oracle.
- fmt + clippy clean across all three crates.

## Next

Stage 2: `i?`/`p?`/`g?` (runtime `sash_is_print`/`sash_is_graph`, `-i`→`i?`
rename, `?`-suffix parser recognizer, 3-backend dispatch). Then Stage 3
(shorthand), Stage 4 (arith-assign form).
