# 471 — The cold line was the bug

DG asked three things: what "four keeps" actually entailed, delete the
`Severity::Deferred` apparatus, and drive the eleven unreachable sites to a
conclusion. The third produced a bug, which is the only part of this worth
remembering.

## What "four keeps" entailed: comments, not tests

`a2967a4` deleted three DD#116 rivals and annotated four keeps with the
property each earns its place by. It added **no tests**. The annotation is
the rubric's requirement and it was met, but naming a property is a claim
about reachability, and a claim nothing executes is the shape this project
keeps getting burned by.

Worse, one of the four comments **contradicts itself**:

> The arm is cold because `type_check` sees few map attests, **not because it
> cannot run.**

That is a spec gap — DG's standing rule is "every line warm by spec coverage
except ICEs first". It was filed as a backstop and should have been filed as
a missing spec. Carried forward.

## `Severity::Deferred`: the whole axis, not just the variant

Defined in `diagnostic.rs`, printed in `print_diags`, filtered in `parse()`
— and **never constructed**. Only `Severity::Error` is ever built. Its
producer was SB4004 (variable-as-command, to be resolved by SSA), which
survives only in `docs/TIMELINE.md`.

With one variant left, the `severity` field is a constant, so the deletion
took the enum and the field with it. Three consequences, all simplifications:
`print_diags`'s match becomes a literal `"error"`, `parse()`'s "a deferred
diagnostic is not fatal" filter becomes `!diags.is_empty()`, and `Diagnostic`
loses a field that never varied. Output is byte-identical.

## The eleven sites, and the one that was a bug

Two of the eleven were `parse/expr.rs:652` and `:662` — the `${name}` arms of
`dynamic_subscript_var`, which turns a subscript index into either a runtime
variable read or a literal key. I had filed them as "not proven" twice, which
in hindsight was the finding trying to get my attention.

`dynamic_subscript_var` has exactly one caller chain, and it ends in the
**arithmetic** parser. So the question "can a `${ … }` index reach it" is
really "does `$(( ${a[${i}]} ))` parse". It does not:

```
echo $(( ${a[$i]} ))        0        # ok
echo $(( ${a[${i}]} ))      SB4035   # stray `]`
echo $(( $a[$i] ))          0        # ok
echo $(( $a[${i}] ))        SB4035   # stray `]`
echo $(( a[${i}] ))         0        # ok
echo ${a[${i}]}             0        # ok
```

Three spellings of one read accept the index and two reject it — against
DG's "all paths must agree", and against SB4035's own message, which lists
`a[i]`, `$a[i]` and `${a[i]}` as the three equivalent spellings.

### Cause

`scan_arith_braced_var` ended a `${ … }` body at the **first** `}`:

```rust
while j < bytes.len() && bytes[j] != b'}' { j += 1; }
```

For `${a[${i}]}` that is the inner one. The body truncates to `a[${i`,
`try_braced_subscript` rejects it for not ending in `]`, it degrades to a
`ParamExpansion` token, and the orphaned `]}` lexes as a stray — SB4035. The
sigil spelling fails identically because the lexer desugars `$a[K]` into
`${a[K]}` before arithmetic sees it, so both routes land on the same scanner.

The bare spelling escaped because it uses `scan_balanced_brackets`, and the
word-context spelling because it uses `balanced_subscript_close` — whose doc
comment says, in as many words:

> It is one function on purpose: the two spellings disagreeing about where a
> subscript ends is precisely the class of bug this seam was introduced to
> remove (DG, 2026-10-03 — "all paths must agree"). **Do not grow a second
> scanner for either caller.**

A second scanner had been grown. It predates that comment, which is why the
comment did not stop it: the instruction was written for the callers it knew
about, and `scan_arith_braced_var` scans `{ … }`, not `[ … ]`, so nobody
reading it felt addressed.

### Fix

The scan was already generic over `[] {} ()` and quoted spans — only its
*entry* was pinned to `[`. Generalising it to `balanced_close(bytes, open)`,
with `balanced_subscript_close` as a thin `[`-only wrapper, let the arith
lexer use the same code. Eight specs (`80.123`–`80.130`) pin all four
spellings, including one program that reads the same cell four ways and
asserts they agree.

Both cold lines are now warm by spec. **The cold line was a true positive
pointing at a bug**, which is the whole argument for the coverage queue: not
that cold lines are untidy, but that an unreachable line is a question about
why.

### Filed, not fixed

A map's *literal* key cannot contain `}` or `{` — `${m["a}b"]}` is SB4057 —
while a runtime key can. The outer `${ … }` body scan in the main lexer has
the same quote-blindness. It is not observable today, because such a key
cannot be declared either, so both halves are consistently broken.
`notes/tasks/map-literal-key-cannot-hold-a-brace.md`.

## The other nine

| site | verdict |
|---|---|
| `type_check.rs` `"strlen" \| "arrlen" => Int` | **keep**, spec fidelity + unit test |
| `lexer.rs` `push_source_char` `_ => 1` | `unreachable!`, variants named |
| `lexer.rs` `logos_input` Err arm | **rival**, deleted; `.expect` + unit test |
| `validate.rs` `"" => return` (command name) | **rival**, deleted |
| `validate.rs` empty `exec` target | **rival**, deleted |
| `validate.rs` `check_arith_reads` `SubscriptAssign` | `unreachable!` + unit test |
| `parser.rs` `None => rest.to_string()` | `unreachable!` + unit test |
| `parser.rs` SB4014 unsupported index | **keep**, spec fidelity + unit test |
| `parser.rs` `parse_assign_op` `_ => Err` | **keep**, loud-vs-silent (already annotated) |

Three of these deserve a note.

**`"strlen" | "arrlen" => Int` flipped from delete to keep halfway through.**
The first reading was "`${#x}` is retired by SB4038, so nothing produces the
operator — rival, delete". Then `transform/pipelines.rs:91` turned up building
an AST `${arr arrlen}` for a synthesized for-loop bound. The operator is
live; the arm is cold only because type-check runs *before* transform. That
is DD#116's third property exactly — "states the rule as specified, not as
the current pass ordering delivers it" — and deleting it would have left a
silent `VarType::String` for an int, waiting for a reordering.

**Two of the deletes were silent `return`s standing in front of a loud
error.** `"" => return` in the command-name check and `if tname.is_empty()`
in the `exec` check both end the check early for a shape SB5011 already
rejects. If either ever ran it would wave an unvalidated command through.
Deleting them does not remove a diagnostic; it *restores* one, because the
fall-through path reports SB5011.

**`check_arith_reads`'s `SubscriptAssign` arm held a correct-looking
recursion.** It is a read walker, SB4020 shadows it, and the statement form
routes through `check_arith_compound`. AGENTS.md already names this exact
shape as the worst one to leave cold — it is how `SsaInst::Test` and
`RedirectPush` escaped the SSA taint pass for their whole lives. Converted to
an ICE that names SB4020, pinned with `#[should_panic]`.

`push_source_char`'s `_ => 1` cannot be reached by a unit test either:
`Scanner::new` takes a `&str`, so the source is valid UTF-8 by the type
system. Per AGENTS.md that is the "do not chase it" case — the arm now names
its byte ranges (`0x80..=0xBF | 0xF8..=0xFF`) so a new case is a compile
error rather than a runtime one.

## Gates

`mk test` 107 + 27 + 20 (was 99 — eight new unit tests). Sweep: native 3943,
c 3725, 13 skipped, **7681 total, predicted exactly**. Zero flips, gate
self-test 248, 16 new ids = 8 specs × 2 modes, no retirements.
