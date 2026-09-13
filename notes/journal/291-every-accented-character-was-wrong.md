# 291 — Every accented character in the language was wrong

`echo café` printed `cafÃ©`.

Not in some edge case. Not under a flag. The plainest possible program,
and it had been producing the wrong bytes for as long as the hand-written
lexer has existed.

## How it surfaced

I was still on `docs-regex-dialect-writeup`, walking the regex dialect
construct by construct against the binary. The manpage needs to say what
`.` matches, so I tried `café =~ ^caf.$`. sash said no. Bash says yes.

Fine — bash matches characters, and I already knew `sash-rt`'s
`chartorune` is an ASCII-only stub, so I expected `.` to match one *byte*
and `^caf..$` to match instead.

`^caf..$` also said no. So did `^caf...$`.

That is the useful moment. A byte engine and a character engine are both
coherent positions, and I had a prediction for each. Getting an answer
that fits *neither* means the model is wrong somewhere further back than
the thing I was testing.

I built a small C harness against the actual `regcomp.c`/`regexec.c` that
`sash-rt` ships. `^caf..$` against `caf\xc3\xa9` returned **1**. The
engine was right. So the subject sash handed it was not `caf\xc3\xa9`.

```
$ sash -o t t.sh && ./t | xxd
00000000: 6361 66c3 83c2 a90a                      caf.....
```

`c3 83 c2 a9`. The two bytes of `é` had each been promoted to a character
and re-encoded: `0xC3` → `U+00C3` → `C3 83`, `0xA9` → `U+00A9` → `C2 A9`.
Textbook double-encoding.

```rust
literal.push(self.source[self.pos] as char);
```

`as char` on a `u8` is a Latin-1 decode. `String::push` is a UTF-8
encode. Put them together and every non-ASCII byte in the source doubles
in width. Three sites: the double-quoted body, the bare-word scanner, and
`$'…'`. Assignments and expansions inherit it because they are built from
words.

Single quotes were correct. `scan_single_quoted` doesn't walk bytes — it
slices the source range and calls `from_utf8_lossy` once. The right
pattern was already in the file, thirty lines above one of the broken
sites.

## Why nobody noticed

Because the corruption is a *function*. The same input always produces
the same wrong output, so everything internally consistent still works:

```sh
v=café
if [[ $v = café ]]; then echo ok; fi     # ok
```

Both sides are mangled the same way, so they're equal, so the test
passes. Comparisons pass. Case dispatch passes. Hashing passes. Sorting
passes. The only observations that can catch it are the ones that leave
the program's own frame of reference: **writing the bytes out**, or
**asking for the length**.

`strlen "café"` returned 7. The string is five bytes. Nothing asked.

This is the same shape as the converter bugs in journal 289 and the regex
bug in 290, and by now it is not a coincidence. What survives in a mature
codebase is precisely the population of bugs that is invisible to the
checks the codebase performs on itself. A test suite that only ever
compares sash to sash cannot see an encoding bug. It takes an oracle —
bash — or a raw byte dump.

AGENTS.md has carried the line "byte-level source processing must use
`utf8_char_len`, not `bytes[i] as char`" since May. It was written about
the source preprocessor that has since been deleted. The lesson was
recorded, the code that prompted it was removed, and the same mistake was
made again in the replacement. A learning written down next to the
incident does not travel to the next incident on its own.

## The fix

A `push_source_char` helper that reads the lead byte, takes the whole
UTF-8 sequence, and decodes it with `from_utf8_lossy` — matching what
single quotes already did. Three call sites.

`$'…'` needed more than a call-site swap. `$'\xc3\xa9'` is *two* escapes
that together spell *one* character, so pushing each result as a `char`
could never work no matter how the pushing was done. It now accumulates a
`Vec<u8>` and decodes once at the end, which makes `$'caf\xc3\xa9'` come
out as `café` — agreeing with bash, where before it produced four bytes
where bash produces two.

## What it leaves behind

The regex dialect is now coherently **byte-oriented**: `é =~ ^.$` is
false, `^..$` is true, because `chartorune` is an ASCII stub and `.`
consumes one byte. Bash, in a UTF-8 locale, matches one character. That
is a real divergence and it goes in the manpage — but it is now a
divergence with a single sentence behind it, rather than a behaviour that
matched no model at all. `strlen` returning the byte count is the same
statement from the other side, and is already documented that way.

Eleven specs, `utf8.1`–`utf8.11`. The three that matter most are the ones
that would have caught it: `utf8.6` (byte length), `utf8.7` (three- and
four-byte sequences, so a future fix can't be tuned to two), and `utf8.3`
— single quotes, the path that was *already right*, pinned so a
well-meaning unification doesn't drag it into the broken pattern. And
`utf8.10` asserts the equality that kept passing throughout, because the
comment next to a passing test is the only place to record why a passing
test proved nothing.

Sweep: 4398 passing, 9 skipped, 0 failing, 0 flips.
