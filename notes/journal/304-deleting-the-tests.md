title: 304 — deleting 546 tests, and the harness that tested a compiler we don't ship

DG set the goal precisely, and the precision is what made the work
tractable:

> delete as many tests as possible that are obsolete, targetting dropped
> behaviors, or are already adequately covered by spec tests. Of those that
> remain, we want to write new spec tests to cover the cases, and then
> delete the newly redundant tests. Those that cannot be replaced, need to
> be rehabilitated, and documented to explain why this is the only way to
> test the cases.

That is a deletion-first instruction, and it inverts the reflex. The Rust
suites had been failing for months and every prior filing about them —
`stale-rust-test-suites`, `audit-rust-test-suites`,
`rust-tests-hang-on-retired-surfaces` — framed the job as *repair*. Read as
repair it was enormous. Read DG's way, a failing test is not work to do; it
is **evidence about the test**. The 218 failures were the cheapest signal in
the tree, and I had been treating them as the expensive part.

All 546 live integration tests are now gone, along with `sash-rt-tests`.
Twenty spec tests replaced them.

## Classifying with evidence, not impressions

The rule I held to: no test gets a category from reading it. Every one got a
measurement.

For the 218 failures, I ran each individually and captured its diagnostic.
125 came back with an explicit retired-surface code — SB4020 assign-in-arith
(38), SB4034 param ops (38), SB4029 case (21), SB4038 splat (13), SB5002
heredoc (10), SB5024 args-at-top (6), SB4050 here-string (5). The other 93
shared fixtures with those, so they grouped in behind them.

For the 328 passing tests I needed something better than my own sense of
"that looks covered". I harvested every fixture in `spec/*.sh` — 2299 of
them — reduced each to a construct signature, and matched the Rust tests
against that corpus. 275 exact matches, 15 near.

| cat | n | basis |
|----:|--:|---|
| 1 | 210 | retired surface, per-test diagnostic |
| 2 | 272 | signature match against 2299 spec fixtures |
| 3 | 19 | live feature, apparently uncovered |
| 4 | 45 | bash-differential |

DG pushed back on two of these, and both pushbacks were requests for
evidence rather than votes — "is there any real value there", "are these
safeguarding from regressions that wouldn't be caught by spec tests". Both
were answerable by measurement, and both measurements changed the answer.

Category 4 dissolved completely. 31 of the 45 *also* assert a hardcoded
literal, and both assertions pass — so the bash comparison is provably
redundant, carried along beside an assertion that already pins the value.
`brush_native_read_herestring_ifs` contains no assert statements at all.
`brush_home_clean` diffs `$(getenv HOME)` against bash's `echo $HOME`: two
different programs, equal by construction, a test that cannot fail.

Category 3 was where I nearly wasted the most effort. Nineteen cases that
looked like genuine gaps. Re-checked against the live compiler, **15 already
had concrete spec ids** and most of the remaining expectations were simply
stale — `command -v` gives SB4001 not SB5011; `echo ${undefined}` is
rejected with SB5031, not the empty string the test expected; `$RANDOM`
produces SB5017, a code that does not exist in sash. Four real gaps out of
546 tests.

## The finding that justifies the whole exercise

Checking category 4 turned up something I was not looking for.
`echo_n_multiple_args` passes its bash-differential. The real compiler
rejects `echo -n` with SB5059.

Both statements are true, because `run_bash_native` called
`sash::codegen::compile_to_object` directly. **`validate` never ran.** The
validator rejects `echo -n`; codegen honours it anyway; the test drove
codegen, so the test passed.

That is not a stale test. It is a harness testing a compiler we do not ship.
I measured how far it went: **49 of the 159 passing `run_bash_native` tests
— 31% — compile programs the real compiler rejects.** SB4030 ×24, SB4014 ×7,
SB5020 ×6, SB5062 ×3, SB5059 ×2, SB5017 ×2, SB4037 ×2.

Every one of those was, in the ledger, a *passing test*. A third of that
suite's green was measuring a pipeline with a pass removed. The filings that
called this "hangs forever" had the symptom; the disease was that it was
**fail-open**. I have put this in AGENTS.md in the imperative, because it is
the only durable lesson here: a new harness must drive the real CLI binary,
never `compile_to_object`.

## What actually got replaced

Four gaps, twenty tests. `cf.u1`–`cf.u3` (classic `until`, and `if` nested
in a `while` behind `||`-chained `[[ ]]`), `g.10`/`g.11` (array regex
filter), `cli.24`–`cli.26` (manifest completeness), `35.10`/`35.11` (map
growth past initial capacity).

Writing `g.10` found a bug rather than covering one: `array a=(*.ff =~ 'RE')`
silently yields three un-globbed literal words. `try_array_regex_filter`
demands element 0 be exactly `"*"`, and anything else falls through to a
plain indexed array with no diagnostic. I filed it instead of fixing it —
`array a=(x =~ y)` is a legitimate three-element literal, so blanket
rejection is wrong, and choosing between generalising the guard and adding a
new SB code is DG's call.

## sash-rt-tests, and the two things it was actually protecting

DG's question deserved a real answer, so I mapped all 60 tests. Most fell to
existing specs: the i64 bounds to 25.5–25.7, the overflow traps to 25.4–25.12,
divide and modulo by zero to cov.68/69, the shape predicates to 93.10–93.12.

Three assert behaviour **no sash program can reach**. `atoi_leading_spaces`
and `atoi_trailing_junk` pin the lenient C-atoi semantics of raw `sash_atoi`
— `"  123"` → 123, `"42abc"` → 42 — but the live string→int path is
`sash_atoi_or` → `parse_strict_i64`, which is strict. Raw `sash_atoi` only
runs behind the DD#93 Part D `i?` gate. Those tests were pinning behaviour
that had quietly stopped being the behaviour.

Two were genuine, and both got specs before the deletion:

- `table_grow_many_entries`. `sash_table_new` floors capacity at 4 and the
  largest map literal in the entire spec corpus has three keys, so
  `table_grow` had *zero* spec coverage. Now 35.10/35.11.
- The DD#77 conversion dispositions. `hint.2`/`hint.3` only ever passed
  well-formed values, so every edge of the strict grammar was untested. Now
  77.10–77.16: leading whitespace and trailing junk rejected whole rather
  than partially parsed, `+42` accepted, `007` decimal not octal, i64::MIN
  exact, and out-of-range input yielding the default at exit 0 — a
  disposition concern, not an overflow trap (DD#77:145–152).

## Three repairs hiding behind each other

With the hanging suites gone, `mk test` ran far enough to fail somewhere
new, twice. Each failure had been masked by the louder one upstream.

A doctest was failing: `validate::check_taint`'s doc comment illustrates the
two-phase taint walk with a four-space-indented block of *sash* source,
which rustdoc dutifully compiled as Rust.

Then `sash-grep-test` went 19/26 red. The failures all read "expected 'cat
bat', got ''" — every matching check empty, every error check fine. That
shape says the program under test does not exist, not that the regex engine
is broken. `contrib/sash-grep.sash` had rotted against two retired surfaces
at once: `while read` (DD#99) and `$(argv 1)`. It had not compiled in a long
time. `contrib/test-regex.sh` built it with `2>/dev/null`, converting one
fatal error into nineteen misleading ones pointing at innocent code. I
hardened the harness to print the compiler's stderr and exit 1.

That swallowed stderr is the same failure mode as the fail-open harness,
scaled down: a check that cannot report the thing it is checking.

## Where it stands

`mk test` is **green, rc=0** — 152 C regex checks, 26 sash-grep checks, 77
units. The spec sweep is 4542 passing, 0 failing, 9 skipped, exactly the +14
I predicted before running it. Step 1 of the completion checklist is a real
gate again for the first time in months, and AGENTS.md no longer tells its
reader to skip it.

The recurring lesson, for the third session running: **re-measure before
implementing**. Nine of nineteen native filings were wrong last time. This
time 15 of 19 "uncovered" cases were covered, most category-3 expectations
were stale, and the suite everyone described as slow and broken was more
precisely *not running the compiler*. A filing is a snapshot of a moving
tree, and nothing ages it.

Left for DG: the array-regex-filter bug's fix approach (generalise the guard
vs. a new SB code), and whether `overflow_tests`' deletion sits right — all
17 of its cases map to spec ids, but it was the only thing exercising
`inner.rs` directly.
