# 321 — Three bugs behind three cold branches

The task was coverage: get to 100% branch coverage, add spec tests, and where
a branch cannot be reached decide whether it is dead code or a guard worth a
unit test. That is bookkeeping work. It produced three runtime/parser bugs,
and in each case the cold branch *was* the bug.

That is worth stating plainly because it is the argument for the whole
exercise. A branch with a zero count is not merely untested. It is a piece of
behaviour no one has ever looked at, and the reason it has no test is
frequently that nobody thought about the case at all — which is also the
reason it is wrong.

## What the counts do not tell you

None of the three were diagnosable from the coverage table. The table said
"this arm never ran". It could not say whether that was because the case is
impossible, because the specs are thin, or because the code on that path is
broken. Every classification took a probe: compile the thing, run it, look at
what actually happened.

The discipline that made this work is one DG set earlier and I had to be
corrected on: **safebash has no goal of being identical to bash**, so a bash
mismatch is not evidence of anything. What replaced it is a better signal
anyway — an inconsistency between two of sash's *own* constructs. Each of the
three bugs below was found by holding two sash spellings of the same idea
side by side and noticing they disagreed.

## 1. Int cells faulted on every positional read

DD#76 stores an int collection element as `{val_ptr: null, val_len: <the
i64>}`. `${a[0]}` worked. `$(join a)` segfaulted.

The two go through different accessors. Keyed reads use `sash_table_get`,
where codegen knows the static type and renders the integer. Splat, join and
`for`-in use `sash_table_value`, which has no type to consult and handed the
null pointer straight out to code that expected a string.

DG asked the question that decided the fix: what is cost-free when the ints
are only used as numbers? That accessor split *is* the answer — the
pure-number path never calls the positional accessor, so rendering the
integer there costs nothing in the case anyone optimizes for.

I had recommended a different option and changed to this one mid-discussion,
for a reason worth recording: the objection to the null check is that null
might be ambiguous. It is not. An empty string element is stored as `("", 0)`
— a real pointer to a zero-length string. Null means int cell and nothing
else. Measuring that flipped my recommendation, and I said so rather than
quietly switching.

`notes/tasks/int-valued-collections.md` was marked **done**. Its gates cover
element reads and element assignment. They never mention splat or join. The
feature was complete against its own acceptance criteria and still
segfaulted on the second thing anyone would try.

## 2. `sash_redirect` closed the fd it had just installed

```rust
let src = into_raw_fd(opened);  // open() returns the LOWEST free fd
dup2(src, fd);
sash_close(src);
```

`dup2(src, fd)` with `src == fd` is a POSIX no-op. It does not close
anything. So the unconditional `close(src)` destroyed the redirect exactly
when the target fd happened to be the lowest free one.

With only 0, 1 and 2 open, that is fd 3 and nothing else. `exec 3> f` left fd
3 closed; `close 3` then reported EBADF; and every subsequent `>&3` silently
fell through to the **real stdout**. Output went somewhere the program did
not ask for, with no error.

The existing spec for this feature, `cl.1`, uses fd 9. `dup2(3, 9)` is a
genuine duplication and the close is correct. One arbitrary choice of file
descriptor in one test is the whole reason this shipped. I found it by
writing `exec 3>` instead of `exec 9>` while probing something unrelated.

## 3. An unterminated `$((` inside `"…"` was evaluated anyway

`echo "$(( 1 + 2"` printed **3**.

Here the cold branch and the bug are the same line. `consume_dq_arith_sub`
ended its scan on `self.pos + 1 < self.source.len()` — running out of source
— and then decided whether to skip the closing `))` using *another* length
test. When the input was truncated, both tests failed together: the loop
stopped and the skip was suppressed, and the function returned its partial
text as though nothing had happened. The length test was doing two jobs and
the failure of one masked the other.

Gating the skip on `depth == 0` separates them. Running out of source with
`))` still owed is now recorded through the Scanner's existing `unterminated`
channel, which `parse` reports first and alone — right for this construct,
since an unterminated `$((` swallows the remainder of the program.

Every sibling already got this right: bare `$(( 1 + 2` raises SB4020, `"${
s"` and `"$( cmd"` both raise SB4057. One scanner out of four was the hole.

## The complexity gate did its job

The fix pushed `consume_dq_arith_sub` to cog=12 against a 3×mean bar of
roughly 8.8, and the pre-commit gate rejected the commit. The bypass exists
and I did not use it. The function next door, `consume_dq_cmd_sub`, already
delegates its loop body to `step_dq_cmd_sub`; extracting `step_dq_arith_sub`
to match brought it under and made the two scanners read alike.

A gate that fires on a genuine bug fix is the case where bypassing is most
tempting and least justified. The refactor took one edit.

## Left for DG

`echo $(( 1 / 0 ))` folds to 0 and prints nothing. `int x=0; echo $(( 5 / x
))` yields 0 **and** prints bash's "division by 0" to stderr. Same value,
same exit code, different diagnostics.

I pinned the current behaviour (cov.138/139) rather than changing it. The
runtime side is deliberate and already spec-pinned by cov.68/69, and the
message is a hand-written bash-compatible string, so this is not an oversight
I get to correct on my own — it is a language-surface question about whether
constant folding is allowed to swallow a diagnostic. `fold_arith_op`
returning `None` for a zero divisor would reconcile them in one line, but
which direction they reconcile is DG's call.

## Numbers

4817 passing, 10 skipped, 0 failing, both modes. Branch coverage was 85.37%
(520 missed) at the start of the cycle; 20 further coverage specs landed
after the last measurement, so the next `mk cov-branch` sets the new mark.

The three bugs are the return on the cycle. The tests are the return on the
next one.
