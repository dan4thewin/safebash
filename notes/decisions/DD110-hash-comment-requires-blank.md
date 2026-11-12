title: "DD#110 — a `#` opens a comment only after a blank"
status: accepted

1. A comment-opening `#` is at source start or immediately follows space,
   tab or newline. Other hashes remain word content; quoting preserves them.
2. A hash glued to `;`, `&`, `|`, `<`, `>` or `(` receives a clear boundary
   error rather than becoming a comment. Expansion-closing `)` and `}` do
   not trigger that special error.
3. Arithmetic still rejects malformed numeric literals containing a hash,
   rather than silently truncating them.

---

# DD#110 — a `#` opens a comment only after a blank

`#` starts a comment when it is at the start of the source, or when the byte
immediately before it is a space, a tab, or a newline. Everywhere else it is an
ordinary word character.

```sh
/bin/echo a#b        # a#b       — interior `#`, part of the word
/bin/echo 16#ff      # 16#ff     — likewise; only `$(( ))` reads this as a base
/bin/echo $s#y       # p#y       — after an expansion, still part of the word
/bin/echo a #b       # a         — a blank before it: comment
#nospace             #           — column 1: comment, no blank needed
```

## Why

The scanner used to break every unquoted word at `#` and hand the rest of the
line to the comment regex. `/bin/echo a#b` printed `a`. There was no
diagnostic — the binary just disagreed with bash.

DD#72 hit exactly this collision inside `$(( ))`, where `16#ff` had to be
rejected rather than silently read as `16`, and fixed it *there*. Its own
comment said the `#` "would start a comment and silently truncate the literal
to `16`" — which was also true outside arithmetic, where nothing was guarding.
That half of the fix was never written.

The obvious repair is to emulate bash's word-boundary rule: a `#` is a comment
only when it begins a token. That requires the scanner to know whether the
*previous token* was word-like, which it cannot answer reliably — `$( … )`
closes on `)`, so a produce-time flag misclassifies `$(p)#z`, and the existing
`last_end` is updated at consume time and goes stale across the peek buffer.

DG's rule replaces that state with a single raw byte: look at `source[pos-1]`.
It needs no scanner state, it is one line to state and one line to check, and
it covers the expansion-adjacent cases (`$s#y`, `${s}#y`, `$((…))#z`, `$(p)#z`)
that the token-boundary model gets wrong anyway. It is also the spelling every
readable script already uses.

## The divergence, and why it is loud

bash also opens a comment after an **operator**: `echo a;# c` prints `a`. sash
does not. Left alone that would surface as `SB5011 command '#' not found`,
which explains nothing. So the scanner records the first `#` glued to one of
`;` `&` `|` `<` `>` `(` and the parser reports it:

```
SB4058  a `#` comment must be preceded by a space — this one is glued to the
        operator before it, so sash reads it as a word, not a comment
```

`)` and `}` are deliberately **not** in that set. They far more often close an
expansion (`${s}#y`, `$(p)#z`) than an operator, and bash agrees a `#` after
them continues the word.

A `#` glued to a preceding *word* (`fi# c`) gets no special treatment: bash
rejects that too, and sash's existing parse error already says the right thing.

## Consequences

- No behavior in the safe subset was relying on the old truncation: it was
  silent data loss, never something a program could depend on.
- DD#72's SB4035 is unaffected. The arith guard runs first, so `(( n = 16#ff ))`
  is still rejected with the base-prefix message.
- Quoted words were always correct (`"a#b"`, `'a#b'`, `a"#"b`) and are unchanged.

## Tests

`spec/comments.sh` cmt.12–cmt.26.
