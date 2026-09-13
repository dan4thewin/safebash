# 284 — The desugar that was three bugs

`$(<file)` is the spelling bash provides *because* it does not fork. That is
the entire reason it exists next to `$(cat file)`. In sash it was implemented
as — precisely — `$(cat file)`, synthesised in `parse_slurp_form` with a
comment saying so. The syntax promised an optimization and delivered its
opposite.

The filing called it P3, "purely the cost." Re-measuring made it P2: a static
binary that execs `/usr/bin/cat` is not static. It acquires a runtime
dependency on the *target* machine that nothing in the source mentions, and
under DD#23 a *compile-time* dependency on the build host too. DD#99 then put
it on the common path — every migrated `mapfile -t` is `s=$(<f); matchall …`.

It was blocked on `sash_read_all`'s 64 KiB silent truncation. That closed
2026-09-13 with DD#107, so it was actually unblocked and nobody had noticed.

## One function, two backends

The decision worth recording is where the logic went. The obvious shape is to
emit open/read/close inline — but that is twice, once per backend, and the two
would drift. Instead: one runtime function, `sash_slurp_file`, and both
backends emit a single call to it.

It does not implement the trailing-newline strip either. It delegates to
`sash_capture_drain`, which is literally the code the general `$( )` capture
uses. If `bug-cmdsub-trailing-newlines` ever changes that rule, it changes
both paths at once, because there is only one path. This is the same move as
routing the five `[[ ]]` operators through the runtime comparators earlier
this session.

The one thing that did need writing: `open(O_RDONLY)` **succeeds on a
directory**. It is `read` that returns `EISDIR`, and that fell through to the
path-less `sash_panic_read` — a correct exit code with a useless message. An
explicit `fstat` check makes the diagnostic name the path.

## The exhaustive matches did their job; the `_` arms did not

Adding `ExprKind::SlurpFile` broke 4 compile sites. Adding
`SsaValue::SlurpFile` broke 2. Six failures, six decisions surfaced — and one
of the six was `ssa_taint.rs`, which is the one that mattered most.

`SsaValue::SlurpFile`'s child is the **path**. The any-tainted-composite rule
would therefore have judged `$(<f.txt)` — a literal path — **clean**. File
contents are external input; that is a hole straight through the taint model,
and it was avoided only because the match refused to compile until someone
answered. It is now in the always-tainted arm next to `ArgvIndex`.

The other six integration points have `_` fall-throughs. Every one of them
failed **silently**:

| walker | silent symptom |
|---|---|
| `expr_to_value` | value became the literal text `$(</tmp/f.txt)`, len 18 |
| `child_values` | path never interned → `no entry found for key` in codegen |
| `check_expr_reads` | SB5050 "assigned but never read" false positive |
| `is_taint_source` | taint held, but via the unspanned backstop, not SB5020 |
| `check_expr_taint` | **`[[ $(<f) = x ]]` became legal** |
| `walk_expr_tree` | subtree never visited |

Each was found by running something, not by the compiler. That asymmetry is
the argument for exhaustive matches stated as plainly as it gets: the four
that could not compile cost minutes; the six that compiled cost the afternoon,
and one of them was a taint hole I introduced myself.

## What caught the hole

Not a test. An 11-case behaviour matrix run against stashed HEAD and against
the change, then `diff`ed. `[[ $(<f) = x ]]` was rejected before and accepted
after — a one-line diff in a 40-line output, and nothing in the existing suite
would have said a word.

Four diffs remained after the fix, all accept/reject-identical: better
messages on missing-file and directory, the in-test case now naming the
construct, and `for-in` moving its complaint from the cmdsub to the loop
variable. That last one is a judgement call, so it went to DG rather than into
my own discretion.

## The third bug, found while looking for a better test

Fail-first was weak: only 1 of 10 new specs failed on revert. That is honest —
most of them pin the *byte-identical* gate, which the fix was required not to
change — but one real discriminator is thin, so I went looking for a sharper
one.

A file named `-n`.

`$(<-n)` desugared to `cat -n`. `cat` parsed it as a **flag**, read stdin,
and the slurp yielded nothing: `attest: s failed the gate`. bash yields the
file's contents. Any filename that reaches a real argv is an argument
injection surface, and the fix eliminates it by construction — a direct
`open` has no argv to inject into.

Looking for a better test found a third bug. That keeps happening.

## Gates

| gate | result |
|---|---|
| no fork, no pipe | 1 execve, zero clone/pipe — was 2 execve |
| byte-identical value | 5 / 1 / 10 / 0 across the four shapes |
| no `/usr/bin/cat` | 0 occurrences, both backends |
| still tainted | spanned SB5020 at the use site |

Fail-first fails exactly 2 per mode (`sl.2`, `sl.11`), as predicted. Sweep:
**4292 passing, 9 skipped, 0 failing, 0 flips** — 4270 + the 22 new rows.

## Postscript: the gate caught two of three fairly

One new `ExprKind` variant means one new arm in every exhaustive dispatcher,
and three of them were then rejected by the complexity gate.

Two were fair. `expr_to_string` was cc 17 — a pre-existing violator,
grandfathered until I touched it, which is the mechanism working. `resolve_expr`
was cog 7, and that was *real* nesting: two `for` loops and an `if let` inside
match arms. Extracting `resolve_each` — one iterator-taking helper that serves
a slice, the values of a pair list, and an `Option` alike — took it to cog 1
and genuinely improved it.

The third was not. `expr_to_value` was cc 13, **cog 1**: thirteen arms, each
one delegating call, zero nesting. Nothing to simplify. I split out
`composite_to_value` to get the arm count down, and the split needs a
`_ => unreachable!()` because Rust cannot say "the rest of the variants".

That is the gate pushing toward a catch-all on the same day this entry argues
catch-alls are the hazard. The cause is a boundary artifact: the dispatcher
exemption is `delta ≤ -15`, and for a pure dispatcher `cog = 1`, so `delta`
cannot reach −15 until `cc ≥ 16`. The rejection fires at `cc > 12.1`. Anything
in between is a pure dispatcher the exemption cannot reach. Nine functions sit
in that band today, all of them dispatchers.

The safety property survived — the *parent* match stays exhaustive in every
case, so a new variant is still a compile error and the helper's `_` is only
reachable by an explicit edit. But it cost indirection for nothing. Filed as
`notes/tasks/complexity-gate-dispatcher-dead-zone.md` with a recommendation
(exempt on `cog ≤ 2` directly, rather than approximating it through delta).
Not changing DG's gate unilaterally.
