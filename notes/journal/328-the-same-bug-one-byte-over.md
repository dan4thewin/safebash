# 328 — the same bug, one byte over

Bug #7 was `#`: a byte in `scan_word_chars`' break set that should sometimes
have been part of the word. Bug #8 is `\`, found forty minutes later, in the
same function, for the same reason.

I found it by being unable to leave the coverage loop alone. Four branches had
been flipped in the previous pass and never specced, so before restarting the
measure I ran them against the fixed binary to check they still behaved. One
was `lexer.rs:285`, the line-continuation arm of `skip_whitespace`, probed with:

```sh
/bin/echo a\
b
```

sash: `SB5011 command 'b' not found`. bash: `ab`.

## Why it is bug #8 and not a wart

My first read was "loud error, not silent — file it and move on", the same
disposition I had just given `/bin/echo a}b`. That was wrong, and the thing
that made it wrong took one more probe:

```sh
/bin/echo start\
/bin/echo second
```

Both halves are valid commands on their own. sash compiles clean and prints
two lines; bash prints `start/bin/echo second`. No diagnostic. The loudness in
the first probe was an accident of `b` not being a command — it is not a
property of the bug.

That is the lesson worth keeping: **a divergence is only "loud" if it is loud
for every input, not for the one you happened to try.** I nearly filed a
silent-wrong-output bug as a cosmetic one because my first example happened to
fail for an unrelated reason.

## Not a fork

DD#64 says the shell reads a backslash in exactly two places, and one of them
is "`\` at EOL — line continuation. Kept (structural)." Two of the three
contexts already implemented it: `skip_whitespace` between tokens,
`scan_dq_escape` inside `"…"`. `scan_word_chars` had no `\` arm at all, so the
byte fell through to the verbatim default and the newline then ended the
statement.

So this is the decision tree's step 1, not step 4 — the rule is stated, the
fix applies it to the third context, and there is nothing to ask. One arm:

```rust
b'\\' if self.source.get(self.pos + 1) == Some(&b'\n') => self.pos += 2,
```

DD#64's "Already compliant (no change needed)" paragraph was right about `a\b`
and wrong about `a\<newline>`; it now says so. The non-EOL divergence stays:
`a\b` is `a\b` here and `ab` in bash, deliberately.

## A harness artifact, not a mode difference

`64.15` — a trailing `\` with nothing at all after it — passed native and
failed c. `_run_sash_c` writes its source with `printf '%s\n'` while the
native path uses `-c "$src"`, so in c mode the backslash acquires a newline
and becomes a continuation. The case is *unreachable* through that harness,
not different, so it is `skip_chk`'d rather than weakened to something both
modes agree on. Skips went 10 → 11.

Worth noting because the failure looked exactly like a backend divergence, and
`--emit-c` was the first place I looked. The emitted C was already correct
(`sash_argv_guard("a\\", 2)`).

Sweep 4966/11/0, was 4953/10/0.
