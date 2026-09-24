# 402 — a conversion the language forbids, in both backends

Second item off the re-ranked coverage queue: `emit_c::emit_capture_end_c`,
7 cold lines. The cold region was one arm:

```rust
match target.var_type {
    VarType::Int => {                                    // cold
        writeln!(e.out, "    {base} = sash_atoi({t}.ptr, {t}.len);", …)
    }
    VarType::String => { … }                             // hot
}
```

`sash_atoi` on captured stdout. That is a string→int conversion with no
disposition — precisely what SB4030 rejects, and what journal 400 spent a
session closing in the front end:

> cannot assign string value to int variable `n` — a string→int conversion
> goes through arithmetic and must carry a disposition

So the backend carried an implementation of the conversion the language
forbids. It could not be reached, but "could not" is the claim that needed
testing, not asserting.

## Asking what input reaches it

`CaptureEnd`'s target comes from `state.define(target_name)` at two sites.
Two of the four callers pass synthesized temps (`__strcap_…`, `__bodycap_…`),
which default to string. The other two — `try_capture_inline` and
`try_capture_condition` — pass the *user's own variable*, so `int n=$(…)`
would be an int-typed capture target if type check let it through.

Probed, roughly fifteen shapes: declaration, reassignment, with args, nested
int functions, a pipeline body, a multi-statement body, inside a function, in
condition position, into an array element. Two results were informative.

`int n=$(f | cat)` reported **SB5020**, not SB4030 — which looks like it
passed type check. It had not: adding `attest n i?` removed the taint error
and SB4030 appeared underneath it. The validator's phase ordering had masked
the type error, and reading the first diagnostic as the whole answer would
have sent me looking for a bug that was not there. (Same shape as the masking
in journal 389 and the SB5050 masking in 257 — it keeps recurring because the
first diagnostic is genuinely *a* correct answer, just not the deciding one.)

`int n=$(f)` with `f() -> int` does compile — and emits no capture at all. The
C shows `n = sash_atoi(__intret_f_ptr, __intret_f_len)`: the `__intret_` path,
not `sash_capture_end`.

## DD#58 settles it

The Option C amendment (2026-08-15, `a8ac707`):

> `$(...)` is now **return-type-polymorphic**: it captures the callee's
> declared result — the integer `return` value for a `-> int` function, or
> captured stdout (a string) for a string/void function.

`CaptureEnd` *is* the stdout half. By DD#58 its target is always a string, and
the int half goes through `__intret_`. The `VarType::Int` arm is residue from
before that amendment, when `int n=$(f)` presumably did capture stdout and
atoi it.

Confirmed by execution as well as by argument: a `panic!` in the arm, full
sweep, **6237 green**. The native backend carries the identical dead arm
(`codegen/dispatch.rs`, `VarSlot::Int`, 4 cold lines) and its panic was never
reached either.

## The native fix was already written

The C side became a `let … else` assertion naming DD#58. The native side
needed no new guard at all, because `VarSlot` already has one, and its
INVARIANT comment already lists this caller:

> `str_ptr`/`str_len` are called only on slots the caller has already
> established are strings — positionals, arrays, **read/capture targets**,
> and the `VarSlot::Str` arm of a `match` on this enum

The `match` in `dispatch.rs` was the odd one out: it re-implemented, for a
caller the accessors already covered, the dispatch the accessors exist to
centralise. Replacing it with `sv.str_ptr()` / `sv.str_len()` removed four
cold lines and consolidated onto the two existing guards in `types.rs` — which
were *also* cold, and which are pure `&self` functions, so a unit test reaches
them trivially.

That is the useful pattern here: the cheapest way to close a cold guard is
often to notice it is the third copy of a guard that already exists somewhere
unit-testable.

## Closed

| site | cold before | after |
|---|---:|---:|
| `emit_c::emit_capture_end_c` | 7 | 0 |
| `codegen/dispatch::emit_capture_end` | 4 | 0 |
| `codegen/types::VarSlot::str_ptr`/`str_len` | 2 | 0 |

Thirteen lines, three of them by unit-testing guards that were already there.

## Gates

Sweep **6237**, predicted 6237 — no spec changes — **0 failing, 0 flips**
against `37c790b`. `mk test` **57**/27/20 (up 3). Capture behaviour verified
byte-identical across both backends on a program exercising a function
capture, a multi-statement capture and an int-function capture.
