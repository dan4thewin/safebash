# 261 — the operand slot: one defect wearing three faces

Resuming the backlog prune at `bare-collection-name-operand-asymmetry`, I
went to re-measure the asymmetry at HEAD and typed `len "hello"` almost by
reflex. It printed `0`.

That is the most idiomatic spelling in shell, and it silently produced a
wrong answer. Pulling on it turned up three faces of a single defect:

| spelling | before | should be |
|---|---|---|
| `len "$s"`, `len ${a[0]}`, `len "lit"` | `0`, silently | 5 / element length |
| `join $a ","` | empty output, and `a` not even counted as read (SB5050 fired) | the joined string |
| `keys @m`, `keys $m` | **ICE**: `undefined arrlen: ${marrvals}` | the map's keys |

## One root cause

`try_len_intrinsic` derives a variable *name* by calling `expr_to_string` —
which is a **source-text pretty-printer**, not a name resolver. It strips one
leading `$` and hands back whatever it rendered. So `"$s"` stringifies to
`"$s"` *including the quote characters*, which is then looked up as a variable
name, misses, and reads as nothing. `len $s` works only by accident: the
formatter happens to strip the sigil.

The validator was supposed to catch exactly this. `len_operand_name` only
matches `ExprKind::Literal`, and its `else { return }` carries the comment
*"resolved elsewhere"*. They are not resolved elsewhere. That comment was the
bypass. `join_operand_name` has the same shape — `_ => String::new()`, then
callers bail on the empty string.

Not deliberate: DD#75 §2 says `len` of an unknown name is a *compile error, no
runtime garbage*. There was zero spec coverage for any quoted operand.

## The fix, and what it is not

New `check_operand_spelling` pass emitting **SB5062**, rejecting exactly the
broken cells and naming the variable in the suggestion:

    error [SB5062] `join` resolves its operand as a variable name, and does not
    resolve this spelling. Use a collection name (`join a SEP`) or a splat
    (`join @a SEP`)

I first tried to put this in `type_check.rs` and backed out: every `TypeError`
is emitted as SB4030 at `lib.rs:247`, so it could not carry its own code.

This is rejection, not repair. `join $a` is now rejected while `len $a` still
works — a real inconsistency, left deliberately. DG's pending decisions (`len`
→ collection-only plus a new `strlen`, and the collection-parameter design)
rewrite this exact surface; encoding a guess about the final spelling now
would just have to be undone.

## Why the spellings differ at all

Chasing "just unify on `@a`" ran into the wall that explains the whole family:
**sash has no first-class collection values.** `@a` is a splat into N values,
not a handle to the array. So `keys` genuinely cannot take it — a splat carries
values, and values have no keys. The accepted sets differ because the builtins
need different things, not because someone was careless.

That same absence is the root of DG's parameterised-function problem: a
function can read *and mutate* a caller's array, but only by hard-coding its
name. You can parameterise over a collection's **contents** (splat, read-only);
you cannot parameterise over its **identity**.

The way out is cheap, and it is cheap for a reason I had to correct myself on.
I first told DG this was a "much larger change" — but I was costing a *runtime*
collection value. sash **inlines every function** (SB5037/DD#27 §5 rejects
recursion precisely because there is no runtime call stack), so a collection
parameter can be a compile-time name binding. Monomorphisation. No runtime
handles, no aliasing, no taint-through-pointers.

## The fourth face

The control spec — pinning every spelling that already worked — failed in
janet, and that turned out to be the most interesting result of the session.

| form | native / c | janet |
|---|---|---|
| `len a`, `keys m`, `join @a` | ✓ | ✓ |
| `len $s`, `len $a`, `len @a` | ✓ | compile error: unknown symbol `$s` |
| `join @a ","` | ✓ | bad slot #1, expected string, got array |
| `join a ","` | `x,y,z` | **execs /usr/bin/join** |

So **no single spelling works uniformly across all three backends** — a fact
that belongs in the spelling design, not after it.

Most of this was already filed in `janet-join-intrinsic-gaps` back in August,
including the coreutils fallthrough; only the `$(len $x)` command form was new,
and it went on as an addendum. All of it is pre-existing, verified against the
pre-change build. The cause is the one that file keeps circling: `emit_janet`
renders from the **AST**, so it never sees the operand resolution that native
and c inherit from SSA.

DG deferred the janet overhaul wholesale, including a guard for the
fallthrough. Guarded 75.38 to `native,c`, following the convention that task
already set.

The `/usr/bin/join` case still deserves a name: a backend that silently execs
an external binary when it does not recognise a builtin is not a feature gap,
it is a safety property. Filed, not forgotten.

## Result

8 specs, 75.31–75.38, fail-first verified (7 red, control green) before the
fix. Sweep: **4639 passing / 40 failing / 459 skipped, zero regressions**
against `272b977`.
