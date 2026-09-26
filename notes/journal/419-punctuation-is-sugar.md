# 419 — Punctuation is sugar

DD#111 §Why states the rule plainly: *"Variables are not magic in sash. `$?`
is not a special parameter the way bash has one; it is punctuation sugar for
the `status` intrinsic, and the same holds for `$$`/`getpid` and friends. The
intrinsic is the real thing."*

Today the compiler believed the opposite. The parser **folded** `$(getpid)`
into a `SimpleExpansion("$")` node — the intrinsic spelling collapsed into the
sugar, not the other way round. This session reverses the first of the three,
`$$`, and builds the two mechanisms the remaining two will reuse.

## I was wrong about what was being asked

I opened by offering DG a "defer" option alongside "do it". That was wrong and
I said so: DD#111 is a **stated design rule**, not a diagnostic-wording
preference. Step 1 of the decision tree — *settled by a source of truth* —
applies, and applies to all three magic variables, not just the convenient
one. The only genuine forks left were mechanism and sequencing, which is what
DG actually ruled on (option **B + assertion**, **unify = true**,
**incremental**).

DD#119 mattered here too. Its "How it started" records the **previous**
attempt at this desugar and the blocker that killed it — `/bin/echo hi > $$`
tripped `redirect_target_is_scalar`/SB4049. DD#119 removed that blocker three
commits ago by replacing the syntactic proxy with "provably non-empty", and
its line 60 lists intrinsics as valid redirect targets. The road was already
cleared; nobody had driven it.

## The typing bug that was waiting

Probing before touching anything:

```
int a=$(getpid)   # compiles, Int
int b=$$          # SB4030 "cannot assign string value to int variable"
```

Two spellings of one value with two types. `$(getpid)` is Int by
`type_check.rs`; `$$` was a String `SimpleExpansion`. This is **identical in
kind** to the `$!` bug fixed in `a875d58`, which was patched by seeding
`intrinsic_int` with `!`. The desugar fixes this one structurally instead of
seeding a third entry — after it, there is no `$$` node left to mistype.

Deliberate consequence: `str c=$$` is now SB4030 and directs you to quote it.
`str d="$$"` still works. That is the same migration `cov.252b` took in
`a875d58`, and `cov.252a` takes it here.

## One table, because they had already drifted

`resolve_nullary_sexpr` mapped `getpid`→`$`, `status`→`?`, `getcpid`→`!` — but
only for the **arith sexpr** path. The word form reached the same SSA slot
through the parser fold. Two encodings of one mapping is how they drifted in
the first place, so per DG's `unify = true` the mapping is now
`magic_intrinsic_var` in `transform/expr_handlers.rs`, and both
`resolve_nullary_sexpr` and `resolve_single_cmdsubst` read it.

That refactor was swept **in isolation** before anything else moved: 6510,
zero failing. A pure refactor should be invisible, and proving it was invisible
is what makes the next sweep's diff attributable.

## The assertion is what makes option B safe

Option B routes construction sites through a helper. A helper only helps at
sites that call it, so `parse()` now ends with `assert_no_magic_sugar`, which
walks the finished program and ICEs on any surviving
`SimpleExpansion("$"|"?"|"!")`. Un-routing a site fires it loudly.

Of the eight `SimpleExpansion` construction sites, exactly **four** can receive
a magic name, and all four are routed. The other four are excluded *by
construction*, not by inspection: two go through `dynamic_subscript_var`, which
requires an alphabetic or `_` first character; `try_braced_splat` only handles
`@`/`*`; and `attest_shorthand_test` builds from bound identifier names.

## The flip check was vacuous

Worth recording, because it is the same shape as the coverage canary.

AGENTS.md's regression recipe selects the baseline with `MAX(rowid)` per
`test_id` at a given `commit_hash`. That is correct only when one sweep exists
at that hash. Here the working tree was dirty across three sweeps, all of
which recorded `commit_hash = cc0b7a9` — so `MAX(rowid)` selected **this run**
and compared it against itself. It reported zero flips, and it could not have
reported anything else.

The tell was the row count: 19538 at one hash, which is 6510 + 6510 + 6518.
Re-run against a rowid-bounded first sweep it is a real check, and passes
honestly: 6510 baseline rows, **0 flips, 8 new, 0 retired**.

**A gate that cannot fail is not a gate.** Flagged to DG rather than silently
patched — the recipe lives in AGENTS.md and the fix is his call.

## Gates

- Sweep **6518**, predicted exactly (6510 + 4 dual-mode specs). Zero failing,
  zero retirements.
- Baseline diff vs the first `cc0b7a9` sweep: 0 flips, 8 new (`spv.5`–`spv.8`
  × 2 modes), 0 gone. `cov.252a` migrated in place.
- `mk test` clean; `cargo clippy --release --all-targets` clean.
- Both `chk` collision checks clean; plan-vs-`chk` clean (`coverage.sh`'s
  legitimate −2 aside).

## Neuter proofs, per mechanism

- **Desugar neutered** → `spv.5`/`spv.6`/`spv.8` fail. `spv.7` survives, so it
  is **labelled in place as a non-proof** rather than quietly kept — same
  treatment as `sint.21` and `14.65`/`14.66`.
- **Assertion neutered** (one site un-routed) → ICE fires.

## Gotchas

- **The complexity gate caught a pre-existing violator.**
  `resolve_single_cmdsubst` was already cc=6 / **cog=6** against a 3×mean_cog
  of 5.62 — grandfathered only because nobody had touched it. My nested
  `if args.is_empty() { if let Some(var) … } }` took it to cog=9 and the
  commit was rejected. Extracting both nested blocks (`nullary_magic_var`,
  `wrong_arity_backstop`) brought it to **cog=5** — *below* where it started.
  That is grandfather-on-modify working exactly as designed: the debt came due
  when the function was edited, and the ratchet moved down rather than sideways.
- **`cargo fmt` collapses `\`-continued Rust string literals onto one line**,
  baking the source indentation into the string. It turned the assertion
  message into `"…SimpleExpansion — a                    construction site…"`.
  Write long diagnostic strings as a single short literal.
- **`extglob` is not enabled in the spec harness** — `misc.sh:150` mentions it
  only in a comment. Use `[[ $out =~ ^\[[0-9]+\]$ ]]`, not `+([0-9])`.
- Inserting a doc comment before a **continuation** `///` line splits the
  block and orphans the first line. Anchor on the first `///`.

## Next, in DG's order

1. **`$!` → `$(getcpid)`.** Add the `"!"` arm; delete the `getcpid` arm of
   `bareword_shortcut_expansion` and `parse_getcpid_word`'s `SpecialWord::
   Returned` fold. Lower risk — no state machine involved.
2. **`$?` → `$(status)`.** The largest step, and it carries the one real trap:
   `validate.rs`'s SB5012 availability state machine finds `$?` reads via
   `find_dollar_question` (2744) and `stmt_uses_status` (2754), and **both
   match only `SimpleExpansion("?")`**. Their doc comment claims to cover
   "`$?` / `$(status)`" — true today only because the parser folds the
   intrinsic into that node. Reverse the direction without moving them and the
   whole state machine goes **silently vacuous**. They must move in the same
   commit, with a neuter proving SB5012 still fires. Confirm first what
   `$(status args…)` does — `resolve_single_cmdsubst`'s new branch requires
   `args.is_empty()` to leave it alone, but nobody has checked what it is.
3. **`drop-intrinsic-int`.** Once `?` is an intrinsic, `TypeEnv::new`'s
   `intrinsic_int` `?` seed is dead. Confirm `#` still needs it.
