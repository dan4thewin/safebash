# 262 — collection parameters, and the taint hole under them

DG said "go", so this is DD#100 built: `=name` at the call site, `A`–`E` in
the callee. The DD was settled in the previous session and the implementation
was meant to be mechanical. It wasn't, and the reason is worth recording: at
almost every step the compiler's *actual* behaviour disagreed with what the
design implied, and the disagreements were only visible by running programs.

The method throughout was to probe before believing. It paid five times.

## The shortcut that reintroduced two wrong answers

`type_check` runs before inlining, so a slot has no kind. The obvious move is
to skip the kind-specific checks when the name is a slot — there's nothing to
check against yet, and the inlined code will be checked later anyway.

I made that change and then measured what it bought:

```sh
fn ks { for k in $(keys A); do echo "k=$k"; done }
array a=(p q)
ks =a          # printed garbage
```

```sh
fn get { echo "${A[x]}" }
array a=(p q)
get =a         # silently printed element 0
```

Both of those are *already* compile errors in sash when written directly
(DD#75 §3 and DD#80). The permissive slot reading un-did two decisions that
exist precisely because the alternative is a silent wrong answer. It isn't a
shortcut; it's a regression that happens not to have a test yet.

So slots get their kinds from their call sites, and call sites that disagree
are rejected. That narrows Part B — a genuinely kind-agnostic helper is now an
error — which I put to DG as a fork rather than deciding. He kept it strict.
Same forward-compatibility argument Part C already makes: loosening later
breaks nothing, tightening later breaks programs.

## Three write paths that never got the memo

`resolve_args_name` is the one substitution hook: a slot name resolves to the
caller's collection, and every read then works. Reads were wired. Writes were
not, and each omission failed differently:

- `A[0]=z` → **ICE**, `undefined array A`
- `read $t A` → no error at all: it filled a function-*local* array named `A`
  and left the caller's collection untouched. The out-parameter form, silently
  doing nothing.
- `attest v in A` → **ICE**, `undefined attest-all array: A` — the deferral
  Part E exists to enable

The ICEs announce themselves. The `read` one does not, and it is the one that
matters most: an out-parameter that silently discards its output is exactly
the failure mode the feature was supposed to prevent.

## `=foo` is not a read, except when it is

Part E says `=foo` passes identity, not a value, so it isn't a read. Applied
literally, the dead-variable check rejected **Part A's own example** — the
DD's motivating snippet didn't compile. `nums` was written and, as far as
SB5050 could see, never read.

The resolution is narrower than either extreme: `=foo` is a read of `foo`
exactly when the callee *reads* that slot. Not when it only writes it — so the
out-parameter-never-read case still fires, which is what Part E asks for. It
took slot-usage analysis with a fixpoint (a slot can be forwarded), but it is
the only rule that is true both before and after inlining.

## The hole that wasn't mine

Then the taint probes. `f =a` on a tainted array compiled and printed the
argument, unattested. I went to fix it and found the front-end pass walks a
function body **once, at its definition**, with the tainted set as of that
point. Which raised an obvious question, and the answer was worse than the
slot bug:

```sh
fn user { for v in @g; do echo "$v"; done }
argv g
user            # compiles; prints unattested argv
```

That reproduces at HEAD. It has nothing to do with DD#100. Scalars survive it
because the SSA backstop re-derives them, but `ssa_taint.rs` says in a comment
that it does **not** re-derive array taint — so arrays fall through both
passes. The gap was documented and filed; what wasn't noticed is that
something was falling *into* it.

I stopped and took it to DG with three options. He answered with a question
that was better than all of them: *would it be solved if an initial pass
covered all non-fn source, then fn was covered in order?*

Mostly — but "in order" isn't enough, because a function can taint a global,
and if the filler is defined after the user, an in-order pass still misses it.
I measured both orderings; both leaked. That needs a fixpoint.

DG's next question removed the need for one: *should we simply forbid a fn
from tainting a global?* It can use a local, attest it, and assign out.

That is the right shape, for a reason the codebase had already half-committed
to: `g[0]=$(arg 1)` inside a function was **already** SB5021. The bulk-fill
path (`argv g`) had simply escaped the same rule. So it isn't new policy — it
is finishing an existing one. And with it in force, the top level is the only
place a global's taint can arrive, so phase 1's set is final before any body
is read: no fixpoint, and no conservative rejection of a call that legitimately
ran before the filler.

I checked the escape route exists before agreeing, for both scalars and
arrays. It does.

## Which left the slots

The one thing ordering can't fix: a slot's taint is a *call-site* fact, so no
arrangement of phases exposes it. DG chose to allow a callee to fill a slot
from outside and propagate the taint back to the caller, rather than ban it as
SB5067 bans the global case — an out-parameter carrying external data is most
of what out-parameters are for. Both directions are now resolved structurally
before the walk, with a call-graph fixpoint so forwarding closes too.

Inward taint is a union over call sites, so a function called with a tainted
collection at one site and a clean one at another is checked as tainted. That
is the same conservatism the slot-kind rule has, which at least makes the two
rules explicable together.

## Numbers

`spec/collparam.sh`, 34 tests: 34 native, 34 c, 17 janet + 17 guarded. The
janet backend has no slot support at all — it renders from the AST and emits
`(show "nums" (string "=" "nums"))`, so every runtime test fails there and
every diagnostic test passes. Filed, guarded, not papered over.

Spec sweep: **zero regressions**. The one pre-existing test that changed was
`scope.sh` 86.23, which pinned `f =nums` as a plain word — DD#100 supersedes
it at exactly that spot, and it now asserts SB5066 instead.

## The gate paid for itself

Thirteen of the functions I touched failed the complexity gate. Reading them
together showed why: I had written the same recursive shape — *iterate
statements, act on this one, work out the enclosing function name, recurse
into `child_bodies`* — five separate times, once per collector. Each copy
buried its real work three or four levels deep, which is exactly what
cognitive complexity charges for.

So the fix was one `walk_scoped_stmts` in `ir.rs` and a per-statement helper
at each site. That is a smaller diff than the code it replaced, and it
removes a class of bug I had already hit once: `collect_global_names` skipped
`StmtKind::Function` with a `continue`, which also skipped its *recursion* —
correct here, but only by accident of statement order. The shared walker
states the intent instead (`in_fn.is_empty()`), so the two facts can't drift
apart.

Worth saying plainly: the gate did not ask me to make the code smaller. It
asked me to stop nesting, and the dedup fell out of answering that.

## Two things found along the way

`[[ $s =~ ^[A-Za-z-]+$ ]]` is rejected as `malformed '[]'`. A literal `-`
first or last in a bracket expression is valid POSIX ERE and bash accepts it.
The message comes from the vendored Plan 9 `regcomp9`, not from sash, so the
fix is a fork DG should pick (patch the engine / normalise the pattern /
document the divergence) rather than something to slip into this commit.
Filed P1.

And the Rust integration suites are substantially red **at HEAD** — 141
failures across six suites, mostly diagnostic-code drift (`expected SB4020,
got SB5020`) and fixtures that predate rules like SB5018. I verified this by
stashing and re-running: identical counts with and without my changes. The
completion checklist says "all tests pass"; right now that is not true of the
tree, independently of this work.

Worse than red: `codegen_tests` does not *finish*. I left it running and came
back 47 minutes later to find five compiled test binaries spinning at 100%.
The harness calls `compile_to_object` directly, bypassing `validate`, so a
pre-DD#99 fixture —

    echo -n "one:two:three" | while read -d: -r word; do ... done

— which the compiler itself rejects (SB5059) instead lowers to a loop that
never terminates. Confirmed at `fff2a1b` with my tree stashed. So step 1 of
the completion checklist has been unachievable for some time and nothing said
so; I have noted it in AGENTS.md and filed the fork (rewrite the fixtures vs.
make the harness validate) rather than deciding it.

The lesson I'd keep: *a test suite that hangs reads exactly like a test suite
that is slow.* Run them under `timeout`.

## What I'd tell the next session

The index is only fresh if you go through the shim. I spent an hour reading
line numbers from a three-hour-stale `.indexnudge-rust.db` because I copied
the `spec.db` recipe — the explicit linuxbrew path — onto the index too. DG
caught it. Nothing broke, because every edit was content-addressed rather
than line-addressed, but that was luck. (DG also noted the shim is a no-op
for non-index databases, so the special-casing in AGENTS.md was pointless
advice to begin with; corrected.)
