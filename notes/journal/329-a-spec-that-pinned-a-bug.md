# 329 — I wrote a spec that pinned a bug

Bug #9 is bug #8's other half, and I introduced the spec that would have
hidden it.

When I fixed `\<newline>` in `scan_word_chars` (journal 328) I added 64.15 to
pin the neighbouring case — a `\` as the very last byte of the source, with no
newline after it:

```sh
run_sash '/bin/echo a\'
[[ $rc = 0 && $out = 'a\' && -z $err ]]; chk 64.15
```

That passed, so I shipped it. It asserts sash's behavior, which I had reasoned
about (DD#64: unquoted `\` is a verbatim byte) and never compared to bash.
bash prints `a`. It discards a trailing backslash at end-of-input, exactly as
it discards one before a newline.

I found it an hour later while probing `lexer.rs:297` for coverage. The probe
snippet was `/bin/echo a \` — the same shape one byte over, reached through
`skip_whitespace` instead of `scan_word_chars` — and this time I ran bash
alongside out of habit. `a` vs `a \`. Which immediately implicated 64.15.

## The rule was already written down

DD#64 says the shell reads a backslash in exactly two places and one of them
is "`\` at end-of-line". The last line of a file is at end-of-line whether or
not it carries a newline. So the fix is not a judgement call — both sites now
treat `\` followed by a newline *or* by nothing as a continuation:

```rust
b'\\' if self.pos + 1 >= self.source.len() => { self.pos += 1; break; }
b'\\' if self.source[self.pos + 1] == b'\n' => self.pos += 2,
```

Eight shapes now match bash. The one remaining divergence is the deliberate
one: `a\b` is `a\b` here, `ab` in bash.

## What went wrong, precisely

The failure was not "I forgot to run bash". I ran bash on seven shapes in the
328 verification table and one of them is even labelled "trailing at EOF" —
but that one was `'/bin/echo a\'` **plus a newline**, because I built the
table with a helper whose inputs all ended in one. The no-newline variant only
appeared later, when I wrote the spec, and by then I was no longer comparing.

So: **the spec was written after the verification, from the implementation,
and asserted what the implementation did.** That is the exact shape of a
regression test that locks in a bug. A spec for bash-compatible behavior has
to be derived from bash, not from a passing run.

The corrective is small and mechanical — when a spec asserts output that bash
also produces, the bash side has to be run for *that* spec's input, not for a
nearby one. I have no way to enforce that except to do it.

## A small dividend

With the fix, 64.15 stopped needing its native-only guard. It was native-only
because `_run_sash_c` appends a newline, so the two harnesses disagreed about
the answer — and they disagreed precisely *because* the answer was wrong. The
correct behavior is the same with or without the trailing newline, so the
mode split dissolved. Skips went back 11 → 10.

That is worth noticing in its own right: a spec that needs a mode guard is
sometimes telling you the behavior is wrong, not that the harnesses differ.

## Coverage, incidentally

The pass that found this also closed six lexer branches: `<<-EOF` (cov.193),
an unterminated backtick and a bare backtick before a newline (cov.194/195),
`<<` at end-of-input (cov.196), a `\` as the last byte of a backtick body
(cov.197), and the two continuation branches at `297`/`1739` via 64.16–64.18.

Branch coverage was 87.33% / 475 cold at the start of the pass, up from
86.76% / 495. Sweep 4983/10/0.

## The gate caught the shape of the fix

The commit was rejected: `scan_word_chars` went 11 → 13 cyclomatic, over the
3×-mean bar. Two new match arms for one rule.

Bypassing would have been wrong, and not only because the gate says so — the
two arms were a symptom. "Is this a line continuation" was written out
longhand in `skip_whitespace` and again in `scan_word_chars`, and the whole
bug was that the second copy was missing a case. Three sites, three chances to
get it wrong, and I had just spent two commits proving that happens.

So it became `take_line_continuation`, one definition, called from both. That
still left cc=12, so the break set came out too as `word_ends_here` — which
is the honest shape anyway: the loop was a decision table wearing a loop's
clothing.

| fn | cc | cog |
|---|---|---|
| `scan_word_chars` | 4 | 5 |
| `word_ends_here` | 7 | 2 |
| `track_subscript_depth` | 4 | 1 |
| `skip_whitespace` | 6 | 6 |
| `take_line_continuation` | 5 | 2 |

Was 13 in one function. Sweep unchanged at 4983/10/0.

The gate did not merely block a commit; it pointed at the duplication that
caused the bug it was blocking the fix for.
