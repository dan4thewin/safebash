# 297 — The right answer for the wrong reason

DD#83 removed here-strings. `cmd <<< WORD` is exactly `producer | consumer` in
sash — no subshell on either side, so a binding the consumer makes still
survives, which is the property that makes `read v <<< "$x"` work in bash and
made it worth keeping. The converter was passing `<<<` through, and the
compiler rejects it (SB4050).

The filing had the mapping. Four rows, one of them a judgement call:

> Prefer `$(sub) <<<` → `sub | cmd` (pipe the command directly) over
> `echo $(sub) | cmd` — fewer moving parts, same bytes.

It is the right rewrite. Both halves of the reason are wrong.

## Not the same bytes

bash's `$(…)` strips trailing newlines; `<<<` then appends exactly one. So
`cmd <<< "$(producer)"` always feeds the consumer a stream ending in a single
`\n`, no matter how many the producer emitted. A direct pipe passes all of
them through.

For the idiom this rewrite exists to serve, that is not cosmetic:

```bash
while read -r l; do echo "[$l]"; done <<< "$(printf 'a\n\n\n')"
```

bash runs the body once. The direct pipe runs it three times, twice on an
empty line.

## And not a preference

`echo $(sub) | cmd` is not the cautious alternative I would have reached for.
It does not compile:

```
error [SB5020] tainted command substitution used without attest —
assign it to a variable and attest before use
```

A tainted `$(…)` cannot be used inline. So there is no choice to make: the
direct pipe is the only form that survives the compiler, and the trailing
newline divergence comes along with it. What the converter owes the reader is a
`# CHECK:` saying so, which it now emits — but only for the `$(…)` word, since
nothing else has the problem.

I find this more interesting than a plain wrong filing. The filing reached the
correct output by a route that doesn't exist, and if I had "verified" it by
checking that the recommended output looked reasonable, I would have shipped
the same answer with no note attached. The measurement that found the real
constraint (`echo $(printf hi)` → SB5020) also found the divergence that needs
documenting. Measuring the *rejected* option is what produced the note.

## One word is not a word

`echo` is the producer for everything else, because sash's `echo` never
word-splits — the bash quoting carries over verbatim rather than having to be
interpreted. One exception:

```bash
cat <<< -n      # bash prints "-n\n"
```

`echo -n` is retired (SB5059). `echo "-n"` compiles and prints **nothing** —
both bash's and sash's `echo` read `-n` as the flag regardless of quoting. So
the literal word `-n` gets `printf "%s\n" -n`. Exactly `-n`: `echo -e` prints
`-e`, and `echo $x` with `x=-n` prints `-n` (sash does no runtime flag
parsing, where bash does — sash is the more faithful one here).

A one-word special case looks like superstition until you try both spellings.

## Two bugs underneath

**`read v <<< "$x"` claimed the target wasn't a plain name.** tree-sitter-bash
puts a here-string redirect *inside* the `command` node. `<` and `>` do not —
they wrap the command in a `redirected_statement`. So every pass that walks a
command's children sees `<<<` as an argument unless it knows not to, and
`read_args` classified it as `ReadArg::Other`, which `read_manual_reason` turns
into "read target is not a plain name — sash binds a declared name, not a
computed one (DD#99)". The target is `v`. `mapfile` had the same hole.

Two passes independently mis-reading the same grammar quirk is the signal that
it wants a named helper rather than two filters, so: `herestring_child`.

**Patch ordering was push-order-dependent.** `apply_patches` sorted by `start`
alone. The here-string rewrite is the first pass to emit two patches at one
offset — a zero-width insertion of the producer at the statement start, and
`convert_read`'s replacement consuming forward from the same byte. When the
replacement sorted first, `pos = patch.end` jumped ahead, then the insertion
set `pos` *backwards*, and the next patch re-emitted source that had already
been consumed. Output garbage, silently.

The fix is `sort_by_key(|p| (p.start, p.end))` — at equal `start`, the
zero-width patch sorts first by construction. That is not a workaround for my
change; it is the total order the algorithm always needed, and it was latent
only because nothing had yet emitted both shapes at one byte.

Worth noting how it would have presented: not as a crash, but as converted
output with a duplicated fragment in it. The kind of thing you'd read three
times and blame on the grammar.

## Composition, not replacement

The structural problem in this rewrite is that it *moves* text — the word
travels from the end of the statement to the front. A whole-span replacement
would have been simplest and would have silently dropped every other
conversion inside the statement (`grep "${a[@]}" <<< "$x"` has work to do on
both sides).

So it is three surgical patches instead: insert the producer at the statement
start, delete the redirect where it sits, and render *only the word* against
its own patch vector (`render_subtree`) so its conversions travel with it. The
consumer is never touched, so everything inside it converts normally.

That also keeps line numbers stable, which matters more than it sounds:
`prefix_lines` is keyed by line, and the multi-line form

```bash
while read -r l; do
  echo "$l"
done <<< "$(cmd)"
```

only ever gains text on the first line and loses text on the last. Every
`# CHECK:` and `# ATTEST:` on the interior lines lands where it should.
