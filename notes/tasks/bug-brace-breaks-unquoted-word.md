---
title: "`{` in an argument position silently opens a block — `echo a{echo z}` prints `a\\nz`, bash prints `a{echo z}`"
status: done
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

## Resolved 2026-09-30 — adjacency (option 2, narrowed)

DG chose to reject rather than to make the brace literal, then narrowed the
scope twice, and both narrowings mattered.

**First narrowing.** The plan was to remove the statement-position group
entirely, so `{` would only ever open a body after a header. That was wrong,
and the spec suite caught it: 7 specs failed (`errors.sh` 55.51-55.56,
`functions.sh` 18.8). The group is **not** inert — it is the only way to put
more than one statement on the right of `||`/`&&`:

```
/bin/false || { echo a; echo b; }
```

The "inert" finding came from testing scoping, redirect and pipe (it does not
scope; a redirect or pipe on it is already SB4021/SB4022 "wrap the compound in
a function") and never testing the operator-operand position. Removal would
have cost a real idiom.

**Second narrowing.** DG: "we don't have a `local` keyword anymore, so
wouldn't that error be appropriate?" — SB5011 is the *generic* DD#23
unknown-command error, not a dedicated one, and `local fn inner { … }` is the
same shape as `echo a{echo z}`. A position-based rule would have replaced a
useful "command `local` not found" with a brace complaint.

**The rule that survived both: adjacency.** A `{` glued to the previous word
is the bug; a spaced one is a deliberate block.

| | before | after |
|---|---|---|
| `echo a{echo z}` | silent `a`/`z` | **SB4071** |
| `echo a}b` | SB4014 "expected expression" | **SB4072** (says why) |
| `local fn inner { … }` | SB5011 | SB5011 (unchanged) |
| `cmd \|\| { a; b; }` | works | works |
| `{ echo z }` | works | works |
| `fn f{…}`, `END{…}`, `if /bin/true a{…}` | works | works |

Implementation: `Scanner::header_depth` (mirroring `arith_depth`), set by
`parse_compound_list` — the single choke point every `if`/`elif`/`while`/
`until` condition passes through. `parse_command_element` no longer lists
`Tok::LBrace` as an unconditional terminator; `brace_after_word` decides.
Inside a header a `{` always terminates (DD#68 — `if /bin/true a{echo y}` is
legal and the brace is adjacent there), so adjacency alone would not have
worked.

SB4072 is separate and closes gate 2: a `}` only reaches `dispatch_statement`
when no enclosing block's stop set claimed it, so it provably closes nothing.

Specs: `spec/brace.sh` `bc.g1`-`bc.g10`. Both diagnostics' advice is pinned by
compiling and running it (`bc.g3`, `bc.g10`), not just asserted as text.

### Known residual, accepted

`echo a { echo z }` (spaced) is still a command plus a block; bash reads three
literal words. Pinned as `bc.g6` so it is a decision on record. A spaced brace
reads as a deliberate block, and a statement-position block is an existing
sash surface bash rejects anyway.

`echo a{b,c}` remains SB5011 rather than bash's `ab ac` — sash has no brace
expansion. Unchanged by this fix; see `convert-brace-forms-passthrough`.

### Follow-on filed

`notes/tasks/convert-brace-forms-passthrough.md` — `sash-convert` emits bash
brace forms verbatim, so a literal `a{b}` now converts to something SB4071
rejects.
