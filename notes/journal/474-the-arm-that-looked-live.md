# 474 — the arm that looked live

DG: *"Ideally, any new ArithOp would ICE until it was explicitly given an
overflow check or an exemption. Let's enact changes/deletions based on all the
reevaluations."*

This session enacted the program the previous three assembled. It is mostly
bookkeeping — ten markers, thirty deletions, six ICEs — but two of the steps
were nearly wrong in opposite directions, and both were caught by the same
habit: **probe before you believe a sentence, including a sentence this
project wrote about itself.**

## The shift that was not a bug

The first thing the `ArithOp` work turned up looked like a serious divergence.
`CheckedArithOp::from_arith` exempts `Shl`/`Shr`, so the obvious question was
whether shifts are checked at all:

| | bash | sash |
|---|---|---|
| `1 << 70` | 64 | 0 |
| `1 << 64` | 1 | 0 |

Byte-identical-to-bash is the project's headline claim, so this read as a
stop-everything finding. It is not one. DD#25's 2026-09-20 addendum is DG's
own ruling: bash's answer comes from the hardware masking the count to six
bits, *"a property of the register width rather than of the arithmetic"*,
which makes `1 << 64 == 1` a wrong answer that looks right — the same
silent-wrong class DD#25 already rejects for overflow. A count ≥ 64 shifts
every bit out. `spec/arithmetic.sh` 25.13/25.14/25.17 pin it, with a comment
that says in as many words: *these DIVERGE from bash deliberately*.

The rule in AGENTS.md — read `notes/decisions/` before changing behaviour that
might be deliberate — is what stopped a "fix" here. Worth recording that it
paid for itself, because the divergence is exactly the shape a zero-tolerance
bug policy trains you to pounce on.

## A new `ArithOp` is now a compile error, which is stronger than an ICE

DG asked for an ICE. A total match delivers something better: a new variant
cannot reach runtime at all, because the build stops. `from_arith` read

```rust
_ => None,
```

which meant a new op would be silently **unchecked** — it would compile, run,
and wrap, and nothing in the tree would say so. It now names all nineteen
variants, carries the marker, and records the exemption for each one. The
exemptions were probed, not assumed:

- `Div` traps on `i64::MIN / -1`; `Mod` cannot overflow (`i64::MIN % -1` is 0);
- `Pow` is checked, but by `sash_pow`, not by `CheckedArithOp` — `2 ** 70`
  aborts;
- `Shl`/`Shr` have no overflow to detect, per the addendum above;
- the comparisons and the bitwise ops cannot leave the range.

Injecting a nineteenth variant now fails the build at **five** sites,
`from_arith` among them.

## The arm that looked live

`cmd_subst_taint` has two guards that were to become ICEs: an unrecognisable
head returning `Tainted`, and a user-function head returning `Clean`. The
first was easy — SB5011 rejects a non-literal command name in every position,
and the two producers that bypass `build_argv` hard-code a `Literal` head.

The second nearly went wrong. A one-line probe says a user function capture is
treated as clean:

```
$(getenv HOME)      -> SB5020   (tainted)
$(cat /etc/hostname)-> SB5020   (tainted)
$(f)                -> compiles (clean)
```

which reads as proof that the `user_functions` arm is live and load-bearing —
ICE it and every program using `$(f)` crashes. The in-source comment claimed
the opposite ("never true at all ... lowered elsewhere"), and after a session
spent finding false comments, the comment looked like the thing that was
wrong.

It was right. Making the arm `panic!` and rebuilding shows `$(f)` compiling
without touching it; the cleanliness comes from somewhere else entirely. The
first run of that experiment produced no panic *and no positive control*,
which is worth nothing — a negative result from a binary you have not proven
is instrumented is just a quiet build. Re-run with the unit test as the
control, it panics at `ssa_taint.rs:607` on demand and stays silent for real
source. Eight capture spellings were then tried against the live panic:
`$(f)`, `$(- f)`, `$(f 2>&1)` and `$(strlen "$(f)")` all compile without
reaching it; `$(f x)`, `$(f | cat)`, `if o=$(f)` and `for x in $(f)` are
rejected upstream (SB5043 / SB5020 / SB4025 / SB5009).

**Surface behaviour agreeing with a hypothesis is not evidence for the
mechanism.** "It compiles clean" and "this arm returns Clean" are different
claims, and only one of them was true.

## The other two comments

Two more suspect comments were checked the same way and came out differently
each time, which is the point:

- `detect_array_iter` — AGENTS.md's claim that its five arms are all
  validate-shadowed was already known false (`_ => None` is warm). Three
  `return None` sites are genuinely shadowed by SB5009 and one by SB4045;
  all four were probed and all four became ICEs naming the rule. The warm
  arm was left alone.
- `collect_expr_var_name`'s redirect arms — the comment says the spill makes
  the `Subscript` and `ParameterExpansion` arms unreachable. Journal 439
  recorded a third producer (`RedirectPush`) that bypasses the spill, so this
  looked like a third false comment. Measured: those arms carry counts of 1
  and 2 across a 7687-check sweep, which is exactly the unit test's own four
  calls. The comment is accurate. It was not "fixed".

## Thirty deletions, and zero new specs

The 42 non-ICE unit tests were classified with per-test evidence last session.
Enacting it deleted **30**. The interesting part is the four that were filed as
needing work first:

- `comment_to_eol` — already spec'd, by `cmt.9`, whose comment is a verbatim
  statement of the unit test's claim ("a comment runs to END OF LINE — a `;`
  inside it is NOT a separator").
- `whitespace_skipped_continuations_swallowed` — already spec'd by 64.11 and
  64.16, both of which name `skip_whitespace` explicitly.
- `substring_redirect_target_names_no_internal_operator` — filed with empty
  evidence and classified `review`. `echo hi > ${s:0:2}` turns out to be a
  real program (SB4030), and chk **14.69** already asserts the exact claim,
  `$err != *'${psubstring'*` included.
- the two `verify-then-delete` parser tests — types.sh:910/998 and chk 93.19.

So the predicted sweep delta was **zero**, not +4. A prediction that changes
once you look is the cheapest possible finding.

Of the six `review` tests, five were kept: they pin cross-API invariants
(`scanner_next_agrees_with_peek_then_next`), pass-separation
(`a_cmdsub_body_reports_nothing_from_the_value_walk`), or desugaring equality
that no spec can observe.

## Ten markers, and the one that mattered

The hand-list "exhaustiveness pins" deleted last session were the only thing
that looked like it held totality on their subjects. Ten functions claiming
totality in prose now carry the marker and are gated: `to_tok`, `atok_to_text`,
`tok_to_text`, `expr_to_string`, `check_expr_taint`, `arith_op_str`,
`arith_brief`, both `emit_ir` renderers, and `ArithExpr::children`.

`children` is the one that mattered. Two functions — `reject_prefix_incdec`
and `validate::walk_arith_intrinsic_calls` — **delegate their own totality
claim to it** ("`children()` is total over `ArithExpr`, so a new variant forces
a decision here"). Neither can enforce what it delegates. If `children` grew a
wildcard, both would silently become partial and both doc comments would
become lies. It had no marker.

Five candidates were rejected on inspection: they use "compile error" in the
*sash-language* sense (`keys <non-map>` is a compile error) rather than about
their own match. `lower_slurp_file_value` says "total over both slot kinds"
and contains no match at all. A marker is a permanent constraint; it should
not be applied by regex.

26 functions are now gated, up from 15.
