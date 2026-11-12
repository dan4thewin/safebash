title: "A map's literal key cannot contain `}` or `{`, but a runtime key can"
status: open
priority: P3

# A map's literal key cannot contain `}` or `{`, but a runtime key can

Found 2026-10-04 while fixing the arithmetic braced-body scanner
(`scan_arith_braced_var`). Not that bug — a separate, narrower one in the
same family, filed rather than fixed so the fix stayed surgical.

## The divergence

A map key is just a string. One built at runtime may contain a brace, and
reads back correctly:

```sh
map m=([k]=v); str k2="a}b"; echo ${m[$k2]}     # compiles
```

A key *written as a literal* may not — neither in the declaration nor in the
read:

```sh
map m=(["a}b"]=v)        # SB4031: must use the [key]=value form
map m=([a}b]=v)          # SB4014: expected expression
echo ${m["a}b"]}         # SB4057: unterminated `"` string
echo ${m["a{b"]}         # SB4057: unterminated `${`
```

`${m["a]b"]}` **does** work, because `balanced_subscript_close` skips quoted
spans — so the subscript scanner is already quote-aware. The failure is one
level out: whatever finds the end of the outer `${ … }` body in the main
lexer counts braces without tracking quotes.

## Why it is low priority

The two halves are consistently broken, so no program can observe an
inconsistency *today*: a brace-bearing key cannot be declared, so there is
nothing for the read to fail to find. It becomes observable the moment the
declaration side is fixed, or if `keys`/`read` ever lands such a key in a map
that source then wants to name literally.

## Shape of the fix

Same as the arithmetic one: route the outer `${ … }` body scan through
`crate::parse::expr::balanced_close`, which is quote-aware and generic over
`[] {} ()`. Check the declaration parser's key scan in the same pass — fixing
only the read would turn a consistent restriction into a real divergence.

Needs specs on both sides, and a check of what bash does with
`m["a}b"]` before committing to accepting it at all; "reject it loudly in
both places" is also a coherent answer and may be the better one, since a
brace in a key is unreadable whatever the parser thinks.
