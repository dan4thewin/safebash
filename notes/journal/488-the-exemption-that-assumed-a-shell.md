# 488 — The exemption that assumed a shell

Three changes, all of them the same shape: the compiler held an opinion it
had no business holding, and in each case the opinion came from assuming
`sash` is a shell.

## The glob exemption

Last session added `head_is_unjudgeable` to `resolve.rs` as part of SB5095:

```rust
/// A literal command head the compiler must not judge, because it is not a
/// path: a glob is expanded at runtime …
fn head_is_unjudgeable(name: &str) -> bool {
    name.contains(['*', '?', '[', '$'])
}
```

Every word of that doc comment is wrong, and I wrote it. DG sent me to the
DDs rather than answering directly, which was the right call — the answer
was already written down twice.

**DD#39 is star-only**: "Star-only glob for files (`*` only, no `?` or
`[abc]`)." So `?` and `[` are not metacharacters in sash at all. My own
evidence had told me this and I had misread it: I ran `/bin/ec?o` and
`/bin/[e]cho`, watched both exec literally and fail 127, and recorded that
as "globs do not expand in head position". It was nothing of the kind. They
failed literally because they *are* literal — there is no glob there to
expand.

DG then settled the last of the four: **the star glob is only used with an
array**. In command position `*` is an ordinary filename byte too. And `$`
can never reach an `ExprKind::Literal` head, because SB5011 rejects a head
carrying an expansion long before resolution — so the fourth character was
guarding a shape that cannot occur.

Four characters, four wrong reasons. The exemption re-opened precisely the
compile-time/runtime asymmetry SB5095 exists to close: `/bin/ech*` compiled
clean and became a runtime 127.

`validate.rs` had the same hole independently, in `is_valid_command`:

```rust
// Glob pattern (e.g. /usr/bin/*) — not a command
if name.contains('*') {
    return true;
}
```

That one is older than SB5095 and worse, because a bare `ech*` fell through
it into the PATH search and `resolve_on_path` was never asked about `ech*`
at all. DG: "`ech*` should be an error as no such should exist (does not run
`/usr/bin/echo`)." Both are gone. `/bin/ech*` is SB5095, `ech*` is SB5011.

Worth naming the self-deception: I verified a *behaviour* (two inputs failed
at runtime) and then attributed a *cause* (globs do not expand) that the
behaviour did not support. The inputs could not distinguish the two
hypotheses. Reading the DD first would have cost one query.

## The quoting that had no spelling

DG: "I'm troubled that sash can't run a path with a space."

It could not. A bare word ends at a space, and a double-quoted head is
SB5011 because interpolation means there is no single name to resolve at
compile time. So `/opt/my tool` was simply unsayable — not rejected with a
reason, just absent from the language.

A single-quoted head now denotes a literal command name. DG scoped it in the
same breath: "No superfluous quotes like `'echo'` — should only be used when
there is no unquoted way to say the command name." So the rule is two-sided,
and the second side needs a test for "could this have been written bare?"

The lexer already owns that question. `word_ends_here` is its answer. Rather
than write a second one, I split out the unconditional byte set:

```rust
pub(crate) fn always_ends_bare_word(b: u8) -> bool { … }
```

and built `has_bare_spelling` on top of it, layering the context-dependent
breaks (`#` at index 0, `+=`, `@{`, `[[`, `]]`) back on.

This is still a *mirror* of lexer logic, and this repository has been bitten
twice by a private fork of a shared walker — a fix lands on the canonical
copy and the duplicate quietly keeps the bug. A comment saying "these agree"
would rot the same way. So the agreement is a test that drives the **real
Scanner**: for every string `has_bare_spelling` calls bare-spellable, scan
that text and assert it yields one token holding the input whole.

The assertion is deliberately one-directional, because the predicate is
biased. Every context-dependent break is counted as a break even where this
particular position would not take it. A false *accept* costs a redundant
pair of quotes; a false *reject* would make a real program unsayable. Only
the second direction is unrecoverable, so the bias runs away from it.

Normalization happens in `parse_command`, not in `validate`, and that is not
a style preference. The rest of the compiler holds "a command head is
`ExprKind::Literal`" as an invariant — `ssa_taint.rs` and `capture_purity.rs`
each carry an `unreachable!` naming SB5011 as its guarantee. A `SingleQuoted`
head that survived parsing would reach `transform` and ICE. Rewriting it to
`Literal` at the single site that builds a head from user source keeps the
invariant true by construction. I checked the other six `Command { … }`
construction sites first: all synthesize a literal from a known name
(`status`, `getcpid`, `attest`, `[[`, the sed desugar, the intrinsic
expansion), so none needs the same treatment.

## The image nobody could read

DG: "I would not expect compilation to even start if the image was
unreadable."

It did. `squashfs_listing` answered an unreadable image with an empty map:

```rust
Err(_) => HashMap::new(),
```

which is a confident wrong answer wearing a true one's clothes. Every path
lookup in that empty map says "not there", so **every command head in the
program** reported SB5095 "no such file". The one real fault — a bad
compiler argument — was buried under a diagnostic per line, each of them
blaming the user's source for a problem in the user's command line.

It is now a usage error on the way in, from `check_flags`, which runs in
`parse_cli_args` before the source file is read. A successful read primes the
cache, so `unsquashfs` still runs exactly once.

The silent arm is gone rather than converted to an `unreachable!`. The deep
path reports the same usage error if it is ever reached first, which keeps
one fault with one spelling and does not require me to prove an ordering I
would rather not depend on.

A nonexistent **directory** sysroot has the identical failure shape, and DG's
ruling was about the shape rather than about squashfs specifically, so it
reports the same way. Applying a stated principle to all its consequences
rather than narrowing it to the case that was asked about.

`cli.31`/`cli.32` had to be rewritten: they asserted one SB5011 per command
head, which is to say they pinned the behaviour DG had just called wrong.
They never tested the listing parser either — an absent image means empty
stdout, so not one line is ever parsed. The `sq.*` block covers that, and now
also exercises the cache priming.

## Numbers

- 9 new specs (`qhead.1`–`qhead.9`), dual-mode: +18. Predicted 8079, got 8079.
- `cli.31`/`cli.32` rewritten, count unchanged.
- `mk test` green; 6 lexer unit tests including the Scanner cross-check.
- Complexity: `check_sysroot` first landed at cc 9 / cog 8, over the 3×
  cognitive bar, and `parse_command` at cog 6. Both split before commit —
  `sysroot_fault` / `image_sysroot_fault` / `dir_sysroot_fault` and
  `parse_command_words`. The gate caught both; I did not.

## Filed

`exec /bin/ech*` reports SB5029 "inline `*` glob is not allowed in command
arguments" alongside the correct SB5095, because the target sits in argv
position. Misleading advice, not a wrong answer — the program is still
rejected. P4, `notes/tasks/exec-target-reports-argument-glob-diagnostic.md`.

## The process failure

Three user turns in the middle of this session produced no tool calls at all.
I had the design settled and kept re-deliberating it instead of writing it.
DG had to prompt twice. The work above took about an hour once I started; the
deliberation cost more than that and produced nothing the first round had not
already decided. When the design is settled, open by editing a file.
