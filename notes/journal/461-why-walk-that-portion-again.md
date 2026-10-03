# 461 — why walk that portion again?

DG, reading three options I had filed with no recommendation:

> *"once you find an error in part of the tree, why walk that portion again?
> seems like that is a recipe for numerous errors"*

He was not picking from my list. He was asking why the list existed.

## What I had filed

`for k in $(keys a) { … }` reported its SB4030 twice. I had traced it to two
walkers that both reach a for-header's `$( )`, tabulated their differences,
concluded they were **not equivalent so neither could be deleted**, and offered
three ways to live with that: skip the word-walk for substitutions, merge the
two and accept a behaviour change, or deduplicate identical diagnostics at the
collection point.

All three take the overlap as a given. DG's question does not.

## The filing was wrong twice

**It was never about for-in headers.** My whole comparison table used `keys`,
and `keys` is the one intrinsic validate permits in a for-header substitution
and rejects nearly everywhere else. Every "reports once" row in that table was
a program that had already been rejected one pass earlier. The duplication is
general to a **command argument**:

| | filed | measured |
|---|---|---|
| `for k in $(keys a) { … }` | 2 | 2 |
| `echo "$(join s X)"` | *untested* | **2** |
| `echo "$(join s X)" "$(join s X)"` | *untested* | **4** |
| `f "$(join s X)"` | *untested* | **2** |

I had picked the one probe whose other positions were all unreachable, and
read the resulting silence as evidence.

**And there were not two walkers.** `check_subscript_in_stmt` was a private
copy of `check_command`'s operand-check tail that also descended into `$( … )`.
`check_stmt` already walks `cmdsub_bodies()` unconditionally and runs the
*full* per-statement check on each inner statement — so the copy's descent was
not a second opinion, it was the same opinion twice. My table of "differences"
was a table of **drift between two copies of one thing**.

That is the third time: journal 441 (`emit_exec_static_argv`, a stale fork of
`build_argv_fixed`), journal 456 (a private `walk_arith_expansions` ending in
`_ => {}`), and now this. I have a memory recorded about exactly this pattern.
I did not recognise it, because the two copies here have *different names* and
I arrived at them from the symptom rather than from the shared behaviour.

## The drift was load-bearing

The copies had diverged in **opposite** directions: `check_len_operand` only in
the fork, `check_int_sink_operands` only in the original. So the fork's unique
check ran only where the fork reached:

```sh
str s="ab"
str j="$(len s)"   # accepted. j was "2".
int n=$(len s)     # accepted.
echo "$(len s)"    # SB4030 — correctly
```

`dd75-len-collection-only-strlen` is `status: done`. Its gate reads *"`len
<scalar>` → compile error naming `strlen`"*, and DG's one-line ruling was
*"make len collection only, add strlen for scalar"*. The byte-length overload
that amendment deleted was still reachable — in the position nothing walked.

So the duplicate diagnostic was not the bug. It was the **only visible symptom**
of a redundant walk whose other half was a silent hole.

## What I did about option (b)

My own filing had warned that merging "may newly reject programs". It did —
exactly one, in a 66-program differential against a pristine build of the
parent commit in a scratch worktree. That one was `str j="$(len s)"`, and
rejecting it is the documented rule being enforced for the first time.

A risk I had flagged in the abstract and never measured turned out to be the
fix's main benefit. The measurement cost one `git worktree add` and four
minutes.

## Where I stopped

The same root cause has a second instance: an assignment and a declaration walk
their *target's* index and never their **value**, so `${a[foo]}` on an indexed
array is a clean SB4030 as `echo "${a[foo]}"` and an **ICE** as
`str v="${a[foo]}"` — *"non-integer literal `foo` reached int lowering;
type_check (SB4030) should have rejected it"*. The compiler saying out loud
that a pass it depends on never ran.

I wrote the one-line fix. It regressed `functions.sh` lift.3, which pins

```sh
str z=${arr[$(f)]}     # accepted, prints q
```

while the same expression as an argument is SB4030. lift.1-lift.3 are a
deliberate block about capture *lifting*, and lift.3's own comment shows its
author considered the case. But DD#80 says an indexed array's index "must
yield a number", and a capture yields one only at runtime — which is the
guarantee DD#80 exists to make statically.

So closing the ICE requires ruling on whether a capture may index an array.
That is a language-surface call with evidence pointing both ways, and it is
DG's. Reverted, filed as `capture-as-subscript-index-position-dependent` (P1)
with three options.

Stopping there was the part I nearly got wrong. The ICE is a bug, the bug
policy is zero-tolerance, and I had a working fix in the tree — it is very easy
to talk yourself into "the spec is the hole" when deleting the spec is what
makes your diff green.

## DG's wider question, still open

Removing the overlap removed every duplicate in the corpus, so no
suppress-after-error guard was needed here. Whether to add one anyway — so a
*future* overlap cannot produce duplicates — is filed as
`suppress-rewalk-of-errored-subtree`. My recommendation there is neither
suppression nor nothing, but a debug-only assertion that a cmdsub body is never
visited twice: this codebase's recurring failure mode is invariants no function
owns, and the three bugs above were all found *because* the duplication was
visible. Suppressing the symptom would have bought silence and kept the hole.

## Numbers

Sweep 7395, predicted 7395, zero failures. `mk test` green. 9 specs
(`types.sh` 75.148-75.156) and 75.128/75.129 tightened from `== *SB4030*` to a
count of exactly 1 — which the filing had asked for and which, had it existed,
would have caught the `echo` case the day it appeared.
