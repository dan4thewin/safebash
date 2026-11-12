title: "getenv: presence handling — capture-condition, or an optional default param"
status: done
completed: 2026-09-12
decided: 2026-09-12
priority: P3
kind: task
depends: []
gates:
  - "DG decides how a script tests whether an environment variable is present/absent. Two candidate forms: (a) allow `$(getenv NAME)` as a capture-condition whose status reflects presence (`if x=$(getenv NAME) { use $x }`), or (b) give getenv an optional second argument as a default (`$(getenv NAME DEFAULT)`, absent → DEFAULT), mirroring `$(arg N DEFAULT)`. Pick one (or both)."

# getenv presence handling

**Current policy (DG, 2026-11-12):** DD101 now requires explicit handling or
discard of every lookup status. The original lookup-status delivery below
remains done; `require-getenv-status-handling` owns the new enforcement.
Historical observations below are not current acceptance gates. In particular,
taint validation does not establish presence, and `||` excludes only
unconditional abort, not checking an unhandled failure.

> **DECIDED (DG, 2026-09-12).** Presence is signalled by the **exit status of
> the lookup**, and there is **no default argument**. See "Decision" at the
> bottom; the two candidate designs above are kept for context only.

DG (2026-09-03), on the DD#88 capture-condition rejecting `$(getenv NAME)`:
"getenv might make sense as a way to handle the requested variable being
absent, or we could handle that with an optional second param to getenv."

Today `$(getenv NAME)` in a condition is SB4025 (DD#88 is external-command
only; getenv is a sash intrinsic). But testing env-var **presence** is a real
need. Two candidate designs:

## (a) getenv as a capture-condition (presence = status)

`if x=$(getenv NAME) { … }` would branch on whether NAME is set: present →
success (capture the value into x), absent → failure. This extends the DD#88
capture-condition to the getenv intrinsic specifically (its exit status becomes
"is it set"). Requires deciding getenv's failure semantics (does an unset var
make `$(getenv)` "fail"? today it is tainted and returns … what? — verify) and
wiring the status into the capture-condition path (currently external-only).

## (b) getenv optional default (`$(getenv NAME DEFAULT)`)

Mirror the DD#60 `$(arg N DEFAULT)` addendum: a second argument is the value
used when NAME is unset. `x=$(getenv HOME /root)` → `$HOME`, or `/root` if
unset. This sidesteps the condition entirely — absence is handled inline. It
does NOT tell you *whether* it was present (only gives a value), so it and (a)
are complementary, not mutually exclusive.

## Recommendation

(b) is the smaller, self-contained change (a new arity for the getenv
intrinsic, like `arg`), and covers the common "default if unset" case. (a) is
the more general "test presence" form but pulls getenv into the
capture-condition machinery. DG to choose one or both.

## Related

- DD#88 (capture-condition — external only; this is the intrinsic extension).
- DD#60 `$(arg N DEFAULT)` — the existing optional-default precedent.
- The taint model: `$(getenv NAME)` is tainted (env is untrusted); a default
  literal is clean — a `$(getenv NAME DEFAULT)` result stays tainted.

## Decision (DG, 2026-09-12)

> "the intrinsic isn't failing, but the lookup of a particular env var
> fails; no default arg; rather `home=$(getenv HOME) || home=/home/dg`"

This rejects the optional-default design outright and adopts status-based
presence. The framing matters: the objection that DD#88/SB4025 assume
intrinsics "can't fail" does not apply, because **the intrinsic is not
failing** — it performs a lookup correctly and reports that the lookup found
nothing. That is a result, not a malfunction.

Target form:

    home = $(getenv HOME) || home = /home/dg

### The machinery already exists

Measured on the release binary — the `||` fallback works today for external
commands and is blocked only for the intrinsic:

| source | result |
|---|---|
| `h = $(/bin/echo hi) \|\| h = /fallback` | `hi` |
| `h = $(/bin/false) \|\| h = /fallback` | `/fallback` |
| `h = $(getenv HOME) \|\| h = /fallback` | **SB4025** |
| `h = $(getenv SASH_NOPE) \|\| h = /fallback` | **SB4025** |

So the work is: give `getenv` a meaningful exit status (0 = present,
nonzero = absent) and let it through the capture-condition / `||` path that
SB4025 currently reserves for external commands.

### RESOLVED 2026-09-12 — see DD#101

DG settled both open points:

- ~~**Uncaught is allowed** (option A: status-only). A bare
  `home=$(getenv NOPE)` compiles, binds `""`, sets a failure status nothing
  reads. Handling is *not* mandatory — taint already forces an `attest`
  before any use, and a second mandatory gate would break every existing
  `x=$(getenv HOME)` for no added safety.~~ Superseded by DD101's
  2026-11-12 explicit handling/discard requirement.
- **Present-but-empty reports success.** `NAME=""` is a successful lookup
  (POSIX `getenv` semantics); only *unset* fails. Without this the
  unset/set-to-empty distinction — the entire point of the feature — stays
  unrecoverable.

No default argument (contrast DD#60's `$(arg N DEFAULT)`).
~~No abort (which would be incompatible with `||`).~~ No abort was the
original status-only choice, not a consequence of `||`. Written up as
**`notes/decisions/DD101-getenv-lookup-status.md`**.

### Correction 2026-09-12: abort is NOT forced ~~(and is self-contradictory)~~

An earlier revision of this file claimed DD#42 forced uncaught absence to
abort. **That was a misreading.** DD#42's sentence —

> Out-of-range `$(argv N)` aborts with a clear error message, consistent
> with integer overflow behavior (DD#26). There is no silent empty return.

— is scoped to **`$(argv N)` out-of-range bounds**, the sentence it follows.
It is a statement about array bounds, not a general principle about intrinsic
results, and it says nothing about `getenv` presence.

<del>

Worse, abort-by-default **contradicts the shape DG chose**:

    home=$(getenv HOME) || home=/home/dg

`||` branches on an exit status. An abort tears down the program, so the
fallback could never fire. Picking the `||` form already rules out abort. No
decision needed — the two are incompatible.

</del>

That argument was overbroad. Handled failure can branch while unhandled
failure aborts; DD101 now chooses the distinct compile-time obligation
to handle or discard lookup status.

### Historical taint measurements, not presence handling

The interesting finding. `getenv` results are tainted (DD#24/#53), so the
value cannot be used at all without an `attest`:

    str home=$(getenv NOPE1)
    echo "[$home]"
    → SB5020 tainted variable $home used without attest

And when the attest predicate rejects the empty string, **absence already
aborts at runtime** — no getenv-specific rule required:

    str home=$(getenv NOPE1)
    attest home [[ -d $home ]]
    → attest: home failed the gate     (rc=1)

~~So `getenv` already sits much closer to the "handling required" model than to
the external-command model. The enforcement simply lives in the taint system
rather than in `getenv`.~~ Value validation and presence handling are separate
obligations under the revised DD101 contract.

**The residual hole is narrow.** An attest predicate that *admits* empty lets
absence through silently:

    attest home [[ $home != /root ]]
    → got:[]     (rc=0)

~~That is a general property of weak attest predicates, not a `getenv` defect.~~
Allowing empty text need not be weak validation; it cannot establish that the
environment name existed. The explicit lookup-status obligation is independent.

**What no predicate can fix:** unset and set-to-empty are indistinguishable —
measured identical output for `NOPE1` unset and `NOPE1=''`. The information
never reaches the program. This, not safety, is the real argument for a
presence mechanism: taint already covers safety.

### Open sub-questions for implementation

1. **Set-but-empty vs unset.** `NAME=""` — does the lookup succeed (status 0,
   value `""`) or fail? Recommend **succeed**: the variable is present, and
   this matches `getenv(3)` returning a non-NULL pointer to an empty string.
2. **Taint across the join.** `$(getenv HOME)` is tainted; `/home/dg` is a
   clean literal. After `a || b`, `home` must be **tainted** — the
   conservative join. Needs a gate; verify the SSA phi does not clean it.
3. `getenv` arity becomes exactly 1 — see
   `notes/tasks/bug-getenv-status-unvalidated.md`, which must land the arity
   check either way.

### Follow-on

`notes/decisions/DD42-getenv-argv.md` needs an addendum recording
status-based presence. DD#42's getenv half is still current (only the
argv/argc half is superseded by DD#60), so the addendum belongs there.

## DONE 2026-09-12 — DD#101 implemented

Measured before the fix, on the release binary:

| source | before | after |
|---|---|---|
| `h=$(getenv HOME) \|\| h=/fallback` | **SB4025** | `/home/dg` |
| `h=$(getenv NOPE) \|\| h=/fallback` | **SB4025** | `/fallback` |
| `h=$(getenv EMPTY) \|\| h=/fallback` (`EMPTY=""`) | **SB4025** | `""` — the fallback correctly does **not** fire |
| `if ! h=$(getenv NOPE) { … }` | **SB4025** | branch taken |

Three changes, one per layer.

**Runtime — none needed.** `sash_getenv` already returns NULL for an absent
name and a pointer *into `environ`* for a present one, even when its value is
empty. The found/not-found flag DD#101 asks for was already at the ABI; it was
simply never read. The consequence stands as DD#101 wrote it, though: the
*pointer* is what separates unset from set-to-empty, never the length, which
is 0 in both cases.

**Codegen — set the status.** New
`value_handlers::emit_getenv_lookup_status` writes `exit_status_var` from
`val_ptr == 0`, and `emit_c.rs` emits the same `__exit_status = ({t}_p == 0)
? 1 : 0;`. This is the only new machinery.

**validate — the SB4025 carve-out.** `is_capture_condition_rhs` gained
`is_getenv_lookup`: a single `getenv` command with **exactly one** argument.
Pinning the arity here as well as in `ssa::value_intrinsic_arity` keeps
`$(getenv)` / `$(getenv A B)` on SB4045 rather than letting a malformed call
in as a condition (dd101.11 / dd101.12).

**The transform needed nothing.** `$(getenv X)` already lowers to
`SsaValue::CmdSubst { argv: [Literal("getenv"), …] }`, so DD#88's existing
`try_capture_condition` arm picked it up unchanged once validate stopped
rejecting it.

### Sub-questions from above, answered by measurement

1. **Set-but-empty succeeds** — dd101.3 pins it. Recommendation adopted.
2. **Taint survives the join.** Verified, and it needed no work: after
   `h=$(getenv X) || h=/fallback`, a use without `attest` is still SB5020
   (dd101.8). The clean literal on the fallback side does not clean the phi.
3. Arity landed with `bug-getenv-status-unvalidated`.

### Also corrected

`spec/capture_cond.sh` **88.13 pinned the old behavior** — `if x=$(getenv
HOME)` → SB4025 — and had to be inverted. It is the one spec DD#101
overturns. The SB4025 advice text was rewritten to name the carve-out, and
88.12 / 88.18 still match on `single external`.

### Found while writing the specs

`spec/capture_cond.sh` and `spec/exit_arg.sh` **never called `tally`**.
`run.sh` swallows per-file exit status (`|| true`) and reads the TSV, so
per-test results were recorded correctly — but neither file printed
PASSED/FAILED, and more importantly neither emitted the `[plan=N, ran=M]`
note. That note is the only signal for a silently-swallowed `chk` (it is what
caught `chk 75.35`). Both now call `tally`.

### Verification

Specs `dd101.1`-`dd101.12` in `spec/capture_cond.sh` (plan 18 → 30), plus the
inverted 88.13. **Fail-first confirmed**: against a build with
`validate.rs`/`value_handlers.rs`/`emit_c.rs` stashed, 9 of 13 fail
(88.13 and dd101.1-dd101.8); dd101.9-dd101.12 pass both before and after —
they are the carve-out-is-narrow controls (`$(len a)`, `$(status)`,
`$(getenv)`, `$(getenv A B)` all still rejected).

`dd101.1`-`dd101.7` are guarded `native,c`: janet emits
`(os/spawn ["getenv" "NAME"])` — it forks an external `getenv` — which is the
already-filed `janet-getpid-class-intrinsics`.

### Complexity-gate note

`try_native_intrinsic` (cc 13 / cog 1, delta −12) tripped the gate once this
commit touched it. delta −12 is just short of the −15 pure-dispatcher
exemption, so it had to come down rather than be exempted. Extracted
`try_nullary_intrinsic` — the seven arms (`epochseconds`, `getppid`,
`getpid`, `random`, `getcwd`, `argc`, `signum`) that need nothing but the
builder, module and extern table. `try_native_intrinsic` is now **7/2** and
the new dispatcher **9/1**. Re-swept after the refactor: identical
5247/38/557, zero flips.
