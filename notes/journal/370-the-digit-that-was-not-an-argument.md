# 370 — the digit that was not an argument

```sh
echo hi 2 > f
```

bash writes `hi 2` to the file. sash printed `hi` to stdout and left the
file empty. It had read the `2` as a file-descriptor prefix and redirected
*stderr*.

bash binds a redirect's fd prefix only when the digits **touch** the
operator. `2>f` is fd 2; `2 >f` is the argument `2` and a stdout redirect.
The parser tested only what kind of token came next:

```rust
let next = scanner.peek_n(1);
if matches!(next.token, Tok::Gt | Tok::DblGt | Tok::GtAmp | Tok::LtAmp | Tok::Lt) {
```

Nothing there looks at where the tokens *are*. Spans were available on both
sides the whole time — `looks_like_assignment`, forty lines up, already uses
exactly this test to tell `x=1` from `x =1`. Same file, same need, same
available data, not applied.

The damage is the bad kind: no error, no warning, wrong output. `echo a 1 >f`
and `echo a 3 >f` were wrong too — any one- or two-digit argument in the last
position before a redirect simply disappeared.

## Coverage found it, but not by covering it

The cold line was in `parse_trailing_redirects`, a *second* copy of the fd
logic. Asking "what input reaches this?" produced `echo hi 2 > f` — and
running it against bash answered a question I had not asked. The bug was in
the *other* copy, `parse_fd_prefixed`, which is fully covered and has been
all along. Covered, exercised, and wrong in every run.

Coverage did not find this by being low. It found it by making me ask what a
line is *for*, and the honest answer had to come from bash.

Both copies now go through `fd_prefix_binds`, which is the only place the
adjacency rule is written down.

## What the fix knocked loose

`x=1 2 >f` used to be SB4046 ("a redirect cannot attach to a variable
assignment") — the parser had eaten the `2`. It is now SB5011, `command '2'
not found`, which is bash's reading of that line too (`2: command not
found`). A better diagnostic arrived as a side effect of no longer
mis-parsing.

## A second cold line, genuinely dead

`parse_split_subscript_index` ended with a check for the closing `]`, error
SB4014. It cannot fire. `looks_like_dynamic_subscript_assign` is the only
route in, and it already requires `peek_n(2)` to be exactly `]`; the other
`looks_like_assignment` arm demands a word *ending* in `]`, which cannot
also end in `[`. Deleted, in favour of the probe-then-consume shape
`parse_regex_mutation` already uses.

Its sibling arm — a non-identifier inside the braces, `m[${k:-z}]=1` — is
live, and was simply untested. Two adjacent cold arms, two different
verdicts. "Cold" is a question, not an answer.

## Specs

`fd.1`–`fd.11` in `commands.sh` (plan 112 → 123): four pin the spaced form
as an argument, five pin that the adjacent form still binds, two pin the
trailing-redirect position. `80.38`–`80.40` in `subscript.sh` (plan 37 →
40) for the dynamic-subscript arms.

Sweep **5689** (5661 + 28), predicted exactly, 0 failing, 0 flips.
Re-run unchanged after `parse_trailing_redirects` was flattened for the
complexity gate (cog 6 → 1, against a 5.88 bar).
