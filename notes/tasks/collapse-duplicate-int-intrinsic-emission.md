title: Collapse the duplicate int-intrinsic emission in native codegen
status: done
priority: P2
kind: task
depends: []
gates:
  - "`try_nullary_intrinsic` has no arm that re-emits a call `int_intrinsic_i64` already emits"
  - "`.text` is unchanged for `echo $(getpid)`, `echo $(argc)`, `echo $(signum)`, `echo $(random)`, `echo $(getppid)`, `echo $(waitstatus)`"
  - "`echo $(status)` still lowers as it does today"
  - "spec sweep green, total unchanged"

# Collapse the duplicate int-intrinsic emission in native codegen

DG, 2026-10-10, on whether `$(wait)` should be readable as a string:

> the aim is that int to string conversions must be unambiguously denoted, so
> `str s="$x"`, the double quotes are the expression of intent. `echo` also
> unambiguously denotes writing a string. In terms of implementation, I'd
> expect a bare `echo $x` where x is an int to essentially compile the same as
> `echo "$x"`; likewise for `$(wait)` — so I wouldn't expect the
> implementation of `wait` itself to be context-aware, I'd expect it to always
> return an int.

**The surface already behaves that way, and so does `wait`.** Measured: `echo
$x` and `echo "$x"` compile to byte-identical `.text`, as do `echo $(wait)`
and `echo "$(wait)"`. `wait` has exactly one producer, `emit_wait_value`,
called by `lower_cmdsubst_as_int` for int context and by
`try_native_intrinsic` for string context, which wraps it in
`emit_int_to_str`. That is the shape DG describes.

**Six of its siblings do not follow it.** `getpid`, `getppid`, `waitstatus`,
`random`, `argc` and `signum` each have their value production written
*twice*, in the same file:

- `int_intrinsic_i64` (`value_handlers.rs:757`) — the int context. Four are a
  `declare_func_in_func` + `call` + `inst_results[0]`; `argc` and `signum` are
  a `declare_data_in_func` + `global_value` + `load`.
- `try_nullary_intrinsic` (`value_handlers.rs:549`) — the string context,
  which re-emits the identical sequence and then calls `emit_int_to_str`.

The two copies agree today. That is the only reason this is a task and not a
bug — it is the same shape as the phantom `kill` builtin (journal 489) and the
`emit_exec_static_argv` fork of `build_argv_fixed` (journal 441): two
hand-maintained answers to one question, with nothing holding them together.

**The C backend does not have the split.** `emit_c.rs` has a single
`try_process_intrinsic_c` covering all eight names, which is evidence that the
native split is incidental rather than forced by the backend.

## The shape to aim at

```rust
match name {
    "epochseconds" => …,  // genuinely string-valued: sash_epochseconds() -> (ptr, len)
    "getcwd" => …,        // genuinely string-valued: out-param stack slot
    _ => int_intrinsic_i64(name, …).map(|v| emit_int_to_str(builder, module, externs, v)),
}
```

Then a name added to `int_intrinsic_i64` is reachable from string context for
free, which is the "one list, one question" property whose absence produced
the `kill` bug.

## Two things to check first

1. **`status` is in `int_intrinsic_i64` but not in `try_nullary_intrinsic`.**
   `echo $(status)` works today (four specs use it), lowered via the
   transform's `stateful_intrinsic_slot` rather than this table. Under a
   blanket delegation it would newly match here. Confirm the transform still
   intercepts it first, or exclude it explicitly — do not assume.
2. **`try_nullary_intrinsic` has no `exit_status_var` parameter** and
   `int_intrinsic_i64` requires one (it is what `"status"` returns). Plumbing
   it through is mechanical but it is a signature change.

## Not in scope

`epochseconds` is `ArithDenotation::Int` yet the runtime returns a string
(`sash_epochseconds() -> (ptr, len)`), so `int t=$(epochseconds)` round-trips
through `lower_value` + `atoi` in `lower_cmdsubst_as_int`'s fallback. Making
it int-first would mean a new runtime entry point, i.e. a sash-rt ABI change.
Worth asking DG separately; it is not needed for the collapse above.

## Done, 2026-10-10

All four gates met. `try_nullary_intrinsic` keeps exactly one arm — `getcwd`,
the only nullary intrinsic that is genuinely string-valued — and otherwise
delegates to `int_intrinsic_i64` + `emit_int_to_str`.

**Check 1 resolved: `status` is safe under the blanket delegation**, and it is
safe for a stronger reason than "the transform intercepts it". The transform's
`nullary_stateful_slot` rewrites a bare `$(status)` to `Var("?")` and
`wrong_arity_backstop` rejects `$(status …)`, so the name cannot reach this
table at all. Rather than exclude it, the delegation plumbs `exit_status_var`
through — which makes the arm *total* over what `int_intrinsic_i64` knows
instead of leaving a latent fall-through to the external-exec path if the
transform's interception ever changes.

**"Not in scope" went in scope.** DG ruled the same day that every int
intrinsic must be int-valued in the runtime and reach a `str` target only
through an explicit `"…"`:

> All the int intrinsics should need an explicit double-quote wrap to assign
> to string var.

So `sash_epochseconds()` became `-> i64` (it had been formatting into a stack
buffer and calling `alloc_cstr`, only for `lower_cmdsubst_as_int` to `atoi` it
straight back), `epochseconds` joined `int_intrinsic_i64`, and the C backend's
arm now matches its `getpid`/`random`/`waitstatus` siblings. That removed the
second of the two would-be exceptions, leaving `getcwd` alone.

The denotation rule itself needed no code — it already held uniformly for all
ten int intrinsics. It is now pinned by `spec/types.sh` `den.1`-`den.22`.
