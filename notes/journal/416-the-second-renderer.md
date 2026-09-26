# 416 — The second renderer

## The bug

DD#119 exempts ints from the non-empty proof: "exempt but still needs quotes
to make a string" (DG). So `int n=5; /bin/echo hi > "${n:?msg}"` is the exact
spelling the rule directs authors to write. It did not work.

The C backend emitted:

```c
sash_require_value(n_len, "x", 1);
__redir_target_1_0_ptr = n_ptr;
```

`n_ptr` and `n_len` are the string slots of a `VarSlot`. `n` is an `i64` and
has neither, so this is a **hard compile error** in the generated C — not a
wrong answer. Native was correct throughout.

`load_var_or_empty_c` built `{base}_ptr` / `{base}_len` without ever
consulting the variable's type. That is the *same fault* as
`latest_var_as_str`, which I fixed one commit earlier in 006fce2 — and these
are the only two functions in the backend that render a slot as text. Fixing
one and not the other was the actual mistake; they now share
`var_slot_as_str_c`, so they cannot disagree about an int again.

Nothing had exercised int + disposition + redirect together, so nothing saw
it. It surfaced only when `$!` became int-typed — `cov.252b` does
`int c=$!; > "${c:?no child}"` — and c-mode failed while native passed. The
differential oracle, for the third time this week.

## Two corrections to my own report

I filed this as broken in *both* backends, with native "silently writing to
stdout". That was wrong, and it was wrong because I misread my own probe:

```sh
sash u.sh -o u.bin && ./u.bin && cat 5 && rm -f 5
```

The `hi` I saw came from `cat 5` at the end of the chain — the redirect had
worked and written the file. Native was never broken. I had reported a silent
wrong answer that did not exist, which is a worse error than missing one,
because it sends the next session hunting a phantom.

The second correction is larger. While probing adjacent forms I found
`int n=0; echo "${n:-7}"` prints **7**, where bash prints `0`, and called it a
stop-everything divergence. It is neither. `transform/expressions.rs:291`
carries the reason in a comment crediting DG, and DD#62's Model B amendment
(2026-07-08) settles it: **sash has no unset state**, so each type's zero
value *is* its unset marker — `""` for a string, `0` for an int. `:-` fires on
the identity element of the subject's type. The rule was working exactly as
designed.

I only avoided "fixing" it because AGENTS.md requires checking
`notes/decisions/` before changing anything that might be deliberate. That
check is the whole reason this session did not introduce a regression into a
deliberate design decision.

## On stopping

I ended the previous stretch by saying I was out of context. DG pushed back:
compaction is in place, and had already fired once in this very session.
He was right. Asking about the fork was legitimate — a bug inside a rule I had
just shipped, with three defensible sequencings. Attaching "I'm out of room"
to it was not, and it converted a reasonable question into a handoff.

## Specs

Four new, in `commands.sh` (14.64–14.67), asserting **file contents** rather
than exit status — a broken redirect's failure mode is to write somewhere
else, which a status-only check cannot see.

Two of the four do not prove this fix, and are labelled in place rather than
left to look like coverage: `:-` on an int is a ternary (DD#62), and an int
cell routes through `lower_value_as_str`, so neither reaches
`load_var_or_empty_c`. 14.64 and 14.67 are the proofs. 14.59 — the `"$n".log`
spec from 006fce2 — fails under the same neuter, which is what demonstrates
the two renderers genuinely share the helper now.

## Result

Sweep 6486 (native 3343, c 3130, skipped 13) = 6482 + 4 new × 2 modes, the
predicted total. Zero failing, zero flips vs `0a9d0d5`. `mk test` 106 across
7 suites.

The `isink.*` retirement the purge announced is expected: that work is
stashed, not deleted, pending this fix landing first (DG's option B).

## Next

Unstash the int-sink work — `exit`/`wait`/`arg` int-only — and land it on
this. Then retire `-t` as a `[[ ]]` test in its own commit. `terminal?` is
dropped as YAGNI (DG), the supporting fact being that sash arithmetic has no
unary predicates at all, so it would have been the first of its kind.

---

# 417 — exit, wait and arg take a number

DG's directive: `exit`, `wait` and `arg` accept **int-type only**. One shared
check rather than three, modelled on `check_string_intrinsic_operands`, which
is its exact mirror — that one rejects an int where a string is required.

`arg` was the worst of the three by a distance. `exit $s` and `wait $s` on a
string coerced through the runtime's atoi to 0, which is at least a
*defensible* wrong answer. `$(arg $s)` silently yielded **argv[0]** — not the
argument you asked for, no diagnostic, no trace.

Three calls from DG:

- **`exit ''` is rejected.** It read as 0, i.e. *success*, for a script that
  by writing `exit` meant to fail.
- **A quoted number is a string**, so `exit "1"` is rejected. Quoting is the
  int→string conversion everywhere else in sash — DD#80 map keys, the string
  intrinsics, DD#119 redirect targets — and it cannot mean the opposite here.
- **Signed decimal literals stay legal**, so `exit -1` compiles and exits 255,
  as in bash.

`$(( … ))` is the accountable conversion, and it is strictly better than the
atoi it replaces: `exit $(( ${s:?bad} ))` aborts on a non-numeric value with
the author's own message instead of quietly reporting success.

## Only slot 0

`arg N default` takes a *string* default in slot 1. The check takes one slot,
not all args, so `$(arg 9 fallback)` is untouched. isink.10 pins that.

## The dependency that justified the sequencing

`arg` is only ever written as `$(arg …)` — a command substitution, which is an
*expression*. Wired the way the operand checks were wired before commit
6013848, this check would have caught `echo $(arg $s)` and missed
`str t=$(arg $s)`, which is the only spelling that keeps the result. The
silent-argv[0] case the directive exists to close would have stayed open
behind a green spec.

That is why DG had the traversal fixed first, in its own commit, rather than
folded in here. The sequencing was his call and it was the right one: this
change is small and verifiable precisely because it sits on a sound
traversal.

## `$!` had to become an int

`wait $!` is *the* idiom, and it broke — `$!` typed as String while
`$(getcpid)`, which `$!` is the sugar for, was already int-returning.
`intrinsic_int` seeded `?` but not `!`. So `wait $(getcpid)` would have passed
a rule that `wait $!` failed, for two spellings of one value. The asymmetry
was the bug, not the rule, and the seed is also a step toward the parked
magic-variable directive.

That change is what exposed the C renderer bug in journal 416 above: `$!`
becoming int made `cov.252b` the first spec anywhere to put an int, a
disposition and a redirect in the same line.

## Specs

Twelve, `isink.1`–`isink.12`. Non-vacuity proved twice over: neutering the
check fails the five rejection specs and leaves all seven positives green;
neutering the `$!` seed alone fails three specs across two files.

## Result

Sweep **6510** (native 3355, c 3142, skipped 13) = 6486 + 12 × 2 modes, the
predicted total exactly. Zero failing, zero flips vs `b251d75`.

## Still owed

`-t` retires as a `[[ ]]` test in its own commit (DG). `terminal?` is dropped
as YAGNI — sash arithmetic has no unary predicates at all, so it would have
been the first of its kind, and `[[ ]]` is already the predicate context.
