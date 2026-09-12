title: "bug: janet emits an intrinsic's `$name` operand as a bare symbol"
status: deferred
deferred-by: DD104
priority: P3
kind: bug
depends: []
gates:
  - "`$(len $s)` and `$(strlen $s)` compile and agree with the `$`-less spelling under janet"
  - "a spec pins the `$` form for the value intrinsics in all three modes"

# bug: janet emits `$name` intrinsic operands verbatim

```sh
str s = abcd
echo $(len $s)
```

| backend | result |
|---|---|
| native | `4` |
| c | `4` |
| **janet** | `l1.janet:8:16: compile error: unknown symbol $s` |

The emitted janet is

```janet
(print (string (length $s)))
```

`$s` is passed through as though it were a janet symbol. It is not one, so
the script does not compile.

## Cause

`emit_janet.rs::cmdsub_data_intrinsic` takes the operand with
`expr_to_literal(&c.args[0])` and interpolates it directly. That yields the
source spelling, sigil included. The other two backends strip the `$` before
resolving the name — `transform/expr_handlers.rs` does

```rust
let s = expr_to_string(arg);
let name = s.strip_prefix('$').unwrap_or(&s).to_string();
```

so they treat `len s` and `len $s` as the same operand, which is what DD#75 §2
intends: the sigil is not the dispatch.

## Scope

Not specific to `len`. Any value intrinsic reaching `cmdsub_data_intrinsic`
with a `$`-spelled operand has the same problem; `strlen` (added 2026-09-12)
inherits it verbatim, which is how it was found.

## Found

2026-09-12, adding `strlen`. Spec `types.sh` 75.58 pins the `$` form and is
guarded `native,c` with a pointer here; unguarding it is the gate.


## Deferred to post-beta (DD#104, 2026-09-12)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** — it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.
