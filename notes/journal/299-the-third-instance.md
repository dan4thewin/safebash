# 299 — The third instance

Journal 297 described a bug shape in the abstract while fixing one case of it.
Journal 298 found a second case by accident, one day later, and said so:
"writing down the general shape is not the same as checking for it."

This session I went and checked. There were four more.

## The shape

A converter handler that replaces its **whole node** and a patch produced
**inside** that node cannot both be applied. `apply_patches` walks the source
emitting each patch in order; an inner patch re-emits bytes the outer
replacement already consumed. The output is silently wrong — not an error, not
a crash, just a corrupted script that the compiler then rejects with a
diagnostic pointing at text nobody wrote.

Eleven minutes of measurement against the live binary, one input each:

| input | output |
|---|---|
| `declare -i n=$?` | `int n=$?$(status)` |
| `declare -a a=("$HOME")` | `array a=("$HOME")$home")` |
| `let "x = $?"` | `(( x = $? ))$(status)"` |
| `mapfile -t a < "$HOME"` | `… < "$HOME" \| read a$home"` |

Four handlers: `convert_declaration`, `convert_command`'s `let` arm,
`convert_mapfile`, and the `trap` arms. All four had been wrong since they were
written. None had a spec.

## Composition, then a backstop

Each site is fixed the way `convert_test_command` was: render the children to
text first, run the handler's own transform over *that*, and tell `walk_node`
not to descend into children already spliced in. `render_range` is the new
shared helper — it renders an arbitrary byte range of a node's subtree, which
is what `convert_declaration` needs, since the part it reuses is "everything
after the flags" rather than a whole child.

That fixes the four. It does not stop the fifth. So `apply_patches` now also
drops any patch contained in an earlier patch's span:

```rust
let mut reach = 0usize;
sorted.retain(|p| {
    if p.start < reach { return false; }
    reach = reach.max(p.end);
    true
});
```

A future handler that forgets to compose will now lose an inner conversion
rather than corrupt the file. That is still a bug, but it is a bug that
produces valid output, and the spec suite can catch it. Corruption is the part
worth making structurally impossible.

## What the audit actually turned up

The corruption was the thing I went looking for. It was not the worst thing I
found.

Fixing the `let` arm meant deciding what `let "x = ${#s}"` should convert to.
`let` is arithmetic. In arithmetic the canonical spelling of an intrinsic is
the DD#58 sexpr — `(strlen $s)` — because `$(strlen $s)` there is SB4020, a
command substitution is not an arithmetic operand. That is the same
context-dependence journal 298 hit with `$?`.

So I checked whether the *existing* `${#…}` conversion knew about arithmetic.
It did not:

```
echo $(( ${#s} + 1 ))      →  echo $(( $(strlen $s) + 1 ))     # SB4020
echo $(( ${#a[@]} + 1 ))   →  echo $(( $(len a) + 1 ))         # SB4020
```

A pre-existing bug with no relation to whole-node replacement: the converter
emitted output that does not compile, for one of the most ordinary things a
bash script does. `length_expansion` now takes the context and picks the
spelling.

## Then the sexpr form did not work either

`${#a[1]}` in arithmetic should become `(strlen ${a[1]})`. That failed to
parse:

```
$(( (strlen ${a[1]}) ))   →  SB4020 expected RParen
$(( (strlen $a[1]) ))     →  SB4035 … use the bare cell form `a[i]`
                             or the braced value form `${a[i]}`
```

SB4035 recommends `${a[i]}`, and `${a[i]}` is rejected. A diagnostic that
directs the user to a form the parser does not accept.

`is_func_arg_start` — which decides whether `(name …)` is a sexpr call or
ordinary grouping — listed `Num | Var | DollarVar | LParen` and omitted
`Subscript`, `BareSubscript`, `ParamExpansion`. Those three are operands and
can never be read as a binary operator, so admitting them adds no ambiguity:
the sexpr path requires a leading `Var`, and a real grouping cannot have a
`Var` juxtaposed with a second operand. `(a[1] + 2)` never enters the path at
all, because the lexer hands back one `BareSubscript` token, not a `Var`.

## And behind *that*, a silent zero

With the parser fixed, `$(( (strlen ${a[1]}) ))` compiled and printed **0**.
The correct answer is 3.

`try_arith_int_intrinsic` rebuilds a `Command` from the sexpr args and hands it
to the ordinary intrinsic builder. `reconstruct_intrinsic_command` handles
`Var`, `DollarVar`, `Literal`, and returns `None` for anything else. Its doc
comment says so plainly: "a complex operand returns `None` (the caller falls
back to the IntFuncCall path)."

The IntFuncCall path looks for a *user* function body. There is no user
function named `strlen`. It returns 0.

This is the `getcpid` bug from yesterday wearing different clothes, and the
same sentence describes both: **a fallback written for one category of name,
reached by a name from another category.** `IntFuncCall` is the right fallback
for an unknown callee, which is why nobody noticed it is the wrong fallback for
a known intrinsic.

Probing the remaining operand shapes found two more reachable zeros —
`(strlen ($s))` (redundant grouping) and `(strlen $s $s)` (wrong arity, no
diagnostic at all). The others were masked by earlier diagnostics, which is
luck, not design.

## The arity check existed. It just could not see this spelling.

`$(strlen a b)` has reported SB4045 all along. `$(( (strlen a b) ))` reported
nothing, because `check_cmdsub_arity` matches `ExprKind::CommandSubstitution`
and an arith sexpr is `ExprKind::Arithmetic`. One mistake, one intrinsic, two
spellings, and only one of them was checked.

I wrote the new check in `type_check.rs` first, where the sibling
"string intrinsic in an arith sexpr" check lives. It worked, and it reported
**SB4030** — because that is the code `type_check` emits for everything. So
`$(strlen a b)` was SB4045 and `(strlen a b)` was SB4030: the identical user
error answering to two codes depending on how it was spelled. That is the
defect I had just finished complaining about, reintroduced in the fix for it.

Moved to `validate.rs` next to `check_cmdsub_arity`, reporting SB4045.

## Count

Six bugs, all pre-existing, none of which had a spec:

1. four converter handlers corrupting their own output
2. `${#…}` converting to a form that does not compile in arithmetic
3. the sexpr parser rejecting the operand SB4035 recommends
4. a cell operand to an arith intrinsic evaluating to 0
5. a parenthesised operand evaluating to 0
6. wrong arity in an arith sexpr accepted silently

14 new specs (57.121–57.128, sx.26–sx.31). Sweep 4465 passing, 9 skipped,
0 failing, 0 flips.

## The thing to take away

The audit was scheduled because journal 298 admitted the general shape had been
written down and not acted on. That was the right call — four instances were
sitting there. But the corruption bugs were the *cheap* find. The expensive
finds were the three layers underneath, each one exposed only because fixing
the layer above forced a question about what the correct output actually is.

`let "x = ${#s}"` could not be fixed without deciding the arithmetic spelling.
Deciding the arithmetic spelling exposed that `${#s}` in `$(( ))` had never
compiled. Emitting the sexpr exposed that the parser rejected it. Fixing the
parser exposed the silent zero. Fixing the silent zero exposed the missing
arity check.

Five bugs in a chain, and the only reason the chain was ever entered is that
someone asked what one line of converted output should say.
