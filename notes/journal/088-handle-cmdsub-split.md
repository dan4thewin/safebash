# Journal 088 — handle_cmdsub intrinsic/exec split

## Goal

Reduce `handle_cmdsub` (`codegen/value_handlers.rs`) from CC 15 to floor.
A-verdict from `r47-abstraction-audit.md:124`. Audit recipe: "Same
itoa+stack-slot pattern as handle_param_expansion (pattern #1)" —
already partially applied via `emit_int_to_str` in Wave 1 (CC 19→15).
Now finish the job by splitting intrinsic dispatch from the exec path.

## Result

| function               | CC |
| ---------------------- | -: |
| `try_native_intrinsic` | 11 |
| `emit_cmdsub_exec`     |  4 |
| `handle_cmdsub`        |  3 |

Parent 15 → 3.

## Refactor shape

```
handle_cmdsub
├─ try_native_intrinsic(name, argv, ...) -> Option<(ptr, len)>
│     // 9-arm dispatch: epochseconds, getppid, getcwd, getenv,
│     // argc, signum, argv, dtoi, itod (with arity guards on the
│     // 4 args-taking ones)
└─ emit_cmdsub_exec(argv, redirects, ...) -> (ptr, len)
      // build argv → pipe → fork →
      //   child: dup2 write→1, [redirects], exec
      //   parent: read loop → wait → DD#55 abort → strip newline
```

## Key insight

The function was two cleanly separated responsibilities:

1. **Compile-time intrinsic dispatch** — 9 names with known signatures
   that emit pure-inline CL (no fork). Each returns `(ptr, len)`
   directly; this maps naturally to `Option<(Value, Value)>` (None ⇒
   not a recognized intrinsic, fall through).

2. **Runtime exec** — ~190 lines of fork/pipe/exec/read-loop/wait/
   DD#55-abort/strip-newline. Self-contained pipeline, just plumb the
   same argv+redirects+exit_status_var through.

The parent becomes a 2-step gauntlet: probe intrinsic, else exec. The
literal-name peek (`argv_values.first()` → `SsaValue::Literal`) stays
in the parent so the intrinsic helper takes the name as a `&str`
(simpler signature, no SsaValue knowledge).

## Why not extract per-intrinsic helpers?

Each intrinsic body is 4–22 lines of CL plumbing. Per-intrinsic
extraction would add a `#[allow(clippy::too_many_arguments)]` (8 args
each) and a function declaration overhead per call — for what is
effectively a 9-entry dispatch table. The `match` body stays cohesive
because all arms share the same Cranelift idiom (declare ref → call →
read result). CC 11 on the dispatcher is below the threshold.

If the table grows past ~15 entries, the per-intrinsic split becomes
worth it (consider a `CmdsubIntrinsic` enum with a per-variant emitter
table, mirroring the `BuiltinCmd` enum approach from journal 058).

## Cleanup

Fixed a pre-existing oddity: the `#[allow(clippy::too_many_arguments)]`
attribute was injected **inside** the multi-line doc comment block:

```rust
/// TRUST: Command names are resolved at compile time. Builtins (echo, printf)
#[allow(clippy::too_many_arguments)]
/// are handled inline without fork. External commands use fork/exec.
```

Rust accepted it but the doc rendering would have been broken. Moved
the attribute to its proper position after the doc block.

## Gates

- `cargo build --release` ok
- `cargo clippy -D warnings` clean
- `cargo test --lib` 12/12
- `spec/run.sh` 291/291

## Files touched

- `sash/src/codegen/value_handlers.rs` — split the 390-line
  `handle_cmdsub` into a 42-line parent + 178-line
  `try_native_intrinsic` + 175-line `emit_cmdsub_exec`. No semantic
  change; behavior preserved bit-for-bit.
- `notes/tasks/r47-abstraction-audit.md:124` — annotated.
