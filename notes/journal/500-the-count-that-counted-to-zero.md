# 500 — The count that counted to zero

`continue N` was never specified. `break 2` was, which is why nobody noticed:
the two statements are lowered by twin handlers that were copies of each other
down to the variable names, and a spec on one of them looks, at a glance, like
a spec on the pair.

The cold line that found it was `stmt_handlers.rs:423` — the `.parse()` inside
`handle_continue_stmt`. Probing what warms it turned up four separate faults
behind that one argument.

## Four

```sh
i=0
while (( i < 3 )) { i=$(( i + 1 )); if (( i == 2 )) { X }; echo $i }
```

| `X` | sash before | bash |
|---|---|---|
| `continue 0` | **silent no-op** — prints 1 2 3 | `continue: 0: loop count out of range`, rc 1 |
| `break 0` | **silent no-op** | `break: 0: loop count out of range`, rc 1 |
| `continue abc` | **meant `continue 1`** — prints 1 3 | `numeric argument required`, rc 1 |
| `continue $k` | **compiler ICE** | works (bash resolves at runtime) |
| `continue 1 2` | extra argument ignored | `too many arguments`, rc 1 |

Three of those are silent wrong answers and one is a crash. All five come from
two lines:

```rust
.map(|a| literal_name_of(a).parse::<usize>().unwrap_or(1))
...
if n > 0 && depth > 0 { … }
block
```

`unwrap_or(1)` is where `abc` became `1`. `n > 0` is where `0` became nothing.
And `literal_name_of` is where `$k` crashed — its own `unreachable!` says
"every caller is a name position an earlier pass constrains", and names three
such positions. This caller was a fourth, and nothing constrained it.

## The rule was already written

SB5081 rejects a `break`/`continue` with no enclosing loop, and its doc says
why in terms that settle the count case without anyone having to decide
anything:

> an empty stack makes them a **silent no-op** — the statement compiles and
> does nothing. bash warns; sash rejects.

`continue 0` reaches the identical no-op by the other half of the same
conjunction. Applying a stated principle to all of its consequences is not a
new decision, so SB5097 is SB5081 for the count: zero, non-literal,
non-decimal, or more than one argument. A count *larger* than the nesting depth
is deliberately **not** an error — the transform clamps with `n.min(depth)` and
so does bash, so `continue 9` inside one loop continues that loop in both.

## The ICE that was wrong

Having fixed validate, the transform's `n > 0 && depth > 0` guard looked
entirely dead, so I collapsed both handlers into one shared `loop_target` and
made the fall-through an `unreachable!()` naming the two rules that now
guarantee it.

It fired immediately — on a *correct* program:

```
`break`/`continue` reached the transform with count 9 and loop depth 0
```

`lower_while` lowers the body **twice**. Pass 1 exists only to discover which
variables the body modifies, its instructions are cleared at line 503, and the
loop context is pushed at line 541 — *after* it. So every `break`/`continue`
inside a `while` is lowered once with an empty stack, and emitting nothing is
the right answer there. `for` pushes before its single pass, which is why the
specs that existed never showed it.

The coverage data had said line 432 was cold, and I had read that as "no
program reaches this". It meant "no *spec* reaches this" — the suite's
`continue` specs are all inside `for` loops. A cold line is a hypothesis; this
one was a hypothesis I nearly turned into a crash for every `while` user in the
process of "removing dead code".

The right shape keeps the fall-through and says why:

```rust
fn loop_target(c: &Command, state: &VersionState) -> Option<(BlockId, BlockId)> {
    let n = c.args.first()
        .map(|a| literal_name_of(a).parse::<usize>().unwrap_or(usize::MAX))
        .unwrap_or(1);
    let depth = state.loop_stack.len();
    if depth == 0 { return None; }          // pass 1 of lower_while
    Some(state.loop_stack[depth - n.min(depth)])
}
```

`usize::MAX` rather than `1` on overflow: a count too large to parse should
clamp like every other too-large count, not silently become 1.

## Un-forked

`break` and `continue` now share `loop_target`. They had been separate copies
of eleven identical lines, and the asymmetry in their *specs* — `break 2`
covered, `continue N` not — is exactly how a fault survives in a copied pair.
That is the third time this session that two copies of one idea were the bug.

## Result

Twelve specs, `cf.lc1`-`cf.lc12`: three that pin the nested and clamping
behaviour byte-for-byte against bash, one for the bare `continue` in a `while`
that documents the pass-1 path, and seven rejections. Sweep **8349**, zero
failures, zero flips, exactly 24 new ids. `mk test` 118.
