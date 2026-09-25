# 406 — A marker for the glued operator

DD#118 Parts A-C removed the *competing* reading of `[[ ab==$x ]]` but not
the divergence. Bash reads that word as a bare word — `-n "ab==$x"`, always
true. Part B made the bash reading an error. sash still split the glued
operator and still answered a comparison, so the two shells still disagreed;
the only thing that changed was that bash's answer was no longer *expressible*.

DG's question was whether a convention could tame it. His proposal: require
the left operand to begin with the `$` sigil, so `$foo==abc` and `$foo==$bar`
are a recognisable template and everything else is rejected.

## The premise checked out

The argument is that a non-vacuous binary test always *has* a sigil. Two
things had to hold for that, and both do. Part C already rejects a test whose
operands are both static, so a surviving binary test carries at least one
variable. And `@a` is not a legal scalar test operand — SB5025 — so the sigil
is always `$`, never `@`. The rule therefore costs nothing Part C had not
already taken.

## Pricing it before writing it

The guard is one condition, so the interesting number was the casualty count.
I enumerated every glued-operator site in the suite by shape: exactly six,
all in `dispatch.sh`'s `dp.mp` block. Four (`mp4`, `mp5`, `mp10`, `mp12`) pin
the multi-part *rejoin* in `build_expr_from_parts` and only incidentally used
a literal subject, so they respelled to `$s==$x$y` without losing anything.
Two (`mp3`, `mp6`) *were* the literal-initial case, so they inverted into
rejection specs. `mp7` (`$a$b==$c$d`) independently keeps the right-side
multi-part rejoin covered, which is what made the respell safe.

## `=~` does not commute

The fix-it text was the one place the rule could have given wrong advice.
`==` commutes, so for `[[ ab==$x ]]` the swap `$x==ab` really is a second
correct fix, and DG asked for both to be offered. `=~` does not — its subject
must stay on the left — so `[[ ab=~$x ]]` gets only the spacing, and says so
explicitly rather than staying silent about why the other fix is missing.

Two diagnostics, one code (SB4069), chosen by operator.

## What the guard is actually on

`parts[0]` must not be `Literal` / `SingleQuoted` / `DoubleQuoted`. Every
other `ExprKind` is an expansion. That is a check on the *first part*, not on
whether the word is wholly static, which is why `a$x==ab` is rejected even
though it contains a variable — the marker has to be at the left edge to be a
marker. `${m[$k]}==a` qualifies, since a subscript is an expansion.

One consequence worth recording: SB4069 is a *parse* error, so it now
precedes SB5086 on `[[ a==a { … } ]]`. Spec `118.21` had been pinning
dispatch vacuity through exactly that shape; it moved to the spaced spelling,
which still reaches the validator.

## Verifying the claim, not the checker

Journal 405 was about an extraction regex that silently truncated a third of
the suite, so this time the check ran first: every one of the 3003 extracted
spec bodies through `sash --emit-ir`, grepping for SB4069. Six hits, and all
six were the rejection specs I had just written. Nothing else in the suite
used the form.

## An incidental finding, filed not fixed

Probing whether `sash-convert` could emit the rejected shape turned up
something unrelated. It cannot — it emits the explicit `-n`, which is bash's
reading. But `[ a=b ]` comes out *unconverted*, and so does `[ ab==$x ]`.

tree-sitter-bash 0.25 cannot parse a `[ ]` test containing a glued `=` at
all: it yields an `ERROR` node with `a=b` read as an assignment prefix and
`]` as the command name. `walk_node` never sees a `test_command`, so the text
passes through verbatim — and it *compiles*, because sash has no `[` builtin
and DD#23 resolves `[` to the external `/usr/bin/[`. The answer is
bash-correct and the manifest discloses the external, so nothing is unsound;
the user simply asked for a conversion and got a silent downgrade to a fork.

The general gap is that `convert.rs`'s `"ERROR"` arm handles one known
grammar hole (`${a[a[0]]}`) and otherwise falls through without a `# MANUAL:`
note. Filed as `convert-silent-passthrough-on-parse-error`. Pre-existing, and
unreachable from any DD#118 code path — the branch is never entered.

Worth noting how it surfaced: the first probe returned SB5050 (dead
variable), masking the real behaviour, exactly as the debugging note predicts.
The probe had to be rewritten to read what it wrote before it said anything
true.

## What Part D does not close

`[[ $x==a ]]` still answers F where bash answers T. The template *is* the
shape bash reads as a bare word, so the divergence is inherent to keeping the
no-space dispatch extension at all — and `dispatch.sh` specs that extension
28 times. DG's call was to keep it and mark it: every *unmarked* shape is now
an error rather than a quiet disagreement.

## Gates

6320 specs, zero failures, exactly the predicted total (6306 + 7×2). Zero
flips against the DD#118 baseline. `mk test` 59/27/20, clippy clean, fmt
clean. Complexity re-measured at cc 3.85 / cog 1.87 — `parse_test_binary_tail`
sits at 11/5 against a 11.55/5.62 bar, which passes but has no room left; the
next arm added there will need it split. Non-vacuity by revert: six specs
fail without the guard.
