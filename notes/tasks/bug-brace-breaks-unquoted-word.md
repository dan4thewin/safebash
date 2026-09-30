---
title: "`{` in an argument position silently opens a block — `echo a{echo z}` prints `a\\nz`, bash prints `a{echo z}`"
status: open
priority: P1
kind: bug
depends: []
gates:
  - "`echo a{echo z}` either prints `a{echo z}` or is rejected — it must not silently run `echo z`"
  - "`echo a}b` prints `a}b`, or `}` is rejected with a message that says why"
  - "`${s}`, `if c { … }`, `f() { … }` and a statement-position `{ … }` are unaffected"
---

## This was filed P2 on a false premise — DG corrected it 2026-09-30

The original filing said: "Unlike the `#` case this is **not** silent: it is a
compile error, so no binary disagrees with bash. That is why it is P2 and not
P1."

DG: "seems like this works: echo a{echo z}". It does, and that sentence is
false. Measured:

```
                        sash                 bash
echo a{echo z}          a  / z               a{echo z}
echo a { echo z }       a  / z               a { echo z }
echo a{:}               a                    a{:}
echo a{true}            a                    a{true}
echo one two{echo z}    one two / z          one two{echo z}
```

`--emit-ir` on the first: **two statements** — `{:kind :command :name "echo"
:args ["a"]}` then `{:kind :group :body [...]}`. A `{` in a command's argument
list silently terminates the argument list and opens an anonymous group. No
separator is required, and the space in `echo a { echo z }` makes no
difference — so this is not the mid-word scanner issue the filing described.

**Silent wrong output, so P1.** I had only checked the error cases and drew the
"always a compile error" conclusion from them.

## Why most spellings still error

The group body has to parse as a command list, and DD#23 resolution rejects a
bare name. So `a{b,c}` gives SB5011 "command 'b,c' not found" and `a{b}` gives
SB5011. The silent set is the bodies that resolve: builtins (`echo`, `:`,
`true`, `false`, `printf`), defined functions, and absolute paths.

## The rest of the surface, measured

```
echo a}b        SB4014 expected expression      bash: a}b
echo a{b        SB4014 expected '}'             bash: a{b
echo }          SB4014 expected expression      bash: }
echo hi{        SB4014 expected '}'             bash: hi{
echo {a,b}      SB5011 command 'a,b' not found  bash: a b
echo a{b,c}     SB5011 command 'b,c' not found  bash: ab ac
x=a{echo z}     SB4014 expected expression      bash: (z} not found)
{ echo z }      runs, prints z                  bash: syntax error
```

Two facts that bear on the fix:

- **sash has no brace expansion.** `{a,b}` is parsed as a group, not expanded.
- **A statement-position `{ … }` is a legal sash surface** that bash rejects
  (bash needs `{ echo z; }`). So a fix cannot simply delete the group.

`spec/brace.sh` is DD#68 brace *blocks* (`if/while/for { }`), not brace
expansion — the collision the original filing worried about is not there. The
real constraint is DD#68: inside a control-flow *header* the `{` must be able
to end the condition command with no separator (`if true { … }`).

## Options

1. **`{` is ordinary text in an argument position** (bash-compatible for
   `a{b`, `a}b`, `a { }`). Trades one silent divergence for another: `echo
   a{b,c}` would go from SB5011 to silently printing `a{b,c}` where bash
   prints `ab ac`, because sash has no brace expansion.
2. **`{` in an argument position is rejected** — "`{` starts a block here;
   quote it as `'{'` for literal text". Never silent, keeps `a{b,c}` an error,
   and leaves room to add brace expansion later. Diverges from bash by
   rejecting, which is the subset's normal posture.
3. **`}` ordinary text, `{` rejected** — the union: `a}b` prints `a}b` as bash
   does (no expansion hazard, since brace expansion needs the `{`), and every
   `{` case is a clear error rather than a silent block.

Either way the group stays legal in **statement** position, and headers are
untouched.

DG decides.
