---
title: The closer that never came
date: 2026-09-14
---

# The closer that never came

Journal 312 settled `scan_by_brace_depth`'s loop guard as a coverage gap:

```rust
while self.pos < self.source.len() && depth > 0 {
```

The length test read 0 hits against 10,820 — every `${` in 4,627 spec tests
closes. That looked like nothing more than a missing test, so I wrote one:

```sh
run_sash 'x=5; echo ${x'
[[ $rc != 0 && -z $out && $err == *SB4047* ]]; chk vm.un5
```

It failed. Not because the diagnostic code was wrong. Because there was no
diagnostic. `echo ${x` compiled, linked, ran, and printed `5`.

## What the guard was hiding

`scan_by_brace_depth` ends like this:

```rust
let content = String::from_utf8_lossy(&self.source[inner_start..self.pos]).to_string();
if self.pos < self.source.len() {
    self.pos += 1; // skip closing }
}
content
```

That `if` **is** the run-off-the-end detection. It knows the closer is
missing. And it silently does nothing about it. The content `x` comes back
looking exactly like the content of a well-formed `${x}`, and everything
downstream treats it as one.

Worse, the same three lines appear in eight places:

| helper                    | closer |
|---------------------------|--------|
| `scan_by_brace_depth`     | `}`    |
| `scan_single_quoted`      | `'`    |
| `scan_double_quoted`      | `"`    |
| `consume_dq_cmd_sub`      | `)`    |
| `skip_balanced_squote`    | `'`    |
| `skip_balanced_dquote`    | `"`    |
| `scan_dq_backtick`        | `` ` ``|
| `scan_ansi_c_quoted`      | `'`    |

So:

```
$ sash -c '/bin/echo "hello' -o t && ./t
hello

$ sash -c "/bin/echo 'hello" -o t && ./t
hello
```

bash rejects every one of these — *unexpected EOF while looking for matching
`"'`. sash printed the string and exited 0.

This is the third instance of the same shape in three days: SB4056's
substituted `true`, `scan_via_logos`'s fall-back to a word scan, and now
eight scan-to-closer helpers that answer "did I find the closer?" by not
asking. The pattern is a guard that is already computing the right condition
and then declining to act on it.

## Why the asymmetry query found it and the specs did not

4,627 tests and not one unterminated quote. That is not an oversight, it is
selection: spec tests are written to exercise behaviour, and an unterminated
quote has no behaviour to exercise. Nobody writes a test for a program they
did not mean to write.

Branch coverage does not care what the author meant. It just noticed that one
side of a comparison had never been true.

## The fix

The lexer had no error channel — `Scanner::next` returns `SpannedToken`, not
`Result`. The one existing precedent is `scan_backtick`, which on a
malformed body emits a `` `backtick` `` sentinel Word that the parser
recognises and rejects. A sentinel per construct across eight sites would
have been eight new tokens and eight new parser arms.

Instead the Scanner grows one field:

```rust
unterminated: Option<(usize, &'static str)>,
```

and one helper that replaces the idiom everywhere:

```rust
fn require_closer(&mut self, open_at: usize, what: &'static str) {
    if self.pos < self.source.len() { self.pos += 1; return; }
    if self.unterminated.is_none() { self.unterminated = Some((open_at, what)); }
}
```

First one wins — an unterminated `"` swallows the rest of the file, so every
later complaint is downstream of it. `parse` checks the field after the main
loop and, if set, returns SB4057 *alone*, ahead of the diags it collected:

```
-c:1:11: error [SB4057] unterminated `"` string — it is never closed before
the end of the source, so everything after it was read as part of it
```

The span points at the opener, not at end-of-source, which is the part you
actually need to see.

An internal representation with no language-surface consequence, so per the
decision tree this was decide-and-report rather than ask. The *outcome* —
reject rather than emulate — was never in question: bash rejects it, and
nobody can be depending on an unterminated quote.

## The regression that was not one

The sweep came back 4,660 passing, one failing: `misc.sh` 104.4.

```sh
run_sash_emit_ir 'x="a\"b"; echo $x'
[[ $rc = 0 && $out == *'\\"'* ]]; chk 104.4
```

Its purpose is to check that `emit_ir` escapes a literal containing a quote
and a backslash instead of letting it close the data-reader string early. Good
test. Bad source — because of DD#64.

DD#64 removes shell-level backslash escaping: inside `"…"` a `\` is a verbatim
byte and there is no `\"`. The DD says so in as many words — `"a\"…` →
`a\` + close. So `x="a\"b"` lexes as three tokens: the string `a\`, the word
`b`, and a `"` that opens a third string running to end-of-source. That third
string is unterminated, and 104.4 had been passing only because unterminated
strings were silently accepted.

The test was correct about `emit_ir` and wrong about how to spell its input.
Rewritten to carry the quote in a single-quoted string, where DD#64 leaves
both the `\` and the `"` as literal bytes:

```sh
run_sash_emit_ir "x='a\\\"b'; echo \$x"
```

`emit_ir` now escapes both, and the assertion holds for the reason it was
written.

## State

Sweep 4,661 passing, 10 skipped, 0 failing. 35 unit tests. SB4057 covered by
`errors.sh` 57u.1–57u.12 (six rejects, one span check, five controls proving
the terminated spelling of each construct still compiles and runs) and
`var_modifiers.sh` vm.un5–un6.

## Also found, not fixed — needs DG

The same query run on `parse/parser.rs:2226` found `wrap_with_redirects`'s
non-empty arm at 0 hits in 18,467 calls. It is reachable, and the path it
feeds drops the redirect on the floor: `(( 1 + 1 )) > f` compiles, exits 0,
and never creates the file, where bash creates it.

That one has a genuine fork — reject it like SB4021/SB4046 do for compounds
and declarations, or implement it with the bracketing machinery
`lower_redirected_stmt` already has. Filed as
`notes/tasks/bug-redirect-silently-dropped-on-noncommand.md` with both
options and a recommendation, for DG.
