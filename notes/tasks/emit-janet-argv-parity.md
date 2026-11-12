title: emit_janet — argv/arg/argc backend parity (DD#60)
status: done
priority: P2
kind: feature
depends: []
gates:
  - "[DONE] $(argc), $(arg N), $(argv)/$(argv N) for-in, array a=$(argv N) work in --janet"
  - "[DONE] argv.sh passes fully under SASH_MODE=janet"

# RESOLVED 2026-07-03

janet-native argv via `(dyn :args)` (= `[script, ARGS…]`, matching C's
argv[0]=progname convention — `os/args` doesn't exist in this janet):
- `$(argc)` → `(string (length (dyn :args)))`.
- `$(arg N)` → `(get (dyn :args) N)`, 0-based, out-of-range aborts (av.t6);
  negative literal is a validator compile error (av.t7).
- `for x in $(argv N)` → `(each x [;(array/slice (dyn :args) (min N (length …)))] …)`
  (clamped so N >= argc → zero iterations, av.s3).
- `array a=$(argv N)` → `(array/slice (dyn :args) (min N (length …)))`.
- New `argv_start_index` helper detects the argv splat at the IR level.

**Bonus fix**: `for x in @a` was emitting `(each x [a])` — iterating the
array as ONE value — for ALL arrays (pre-existing janet bug). `emit_for`
now splices `@a` via `janet_arg_splat` → `;a`. Also added a janet `join`
handler (`$(join @a)` → `(string/join a " ")`, `join SEP @a` → sep).

Net: janet failing 106 → 82 (−24), zero regressions. Fixed argv.sh in
full plus cascade wins in control_flow (cf.f3/f4/f7), wordsplit (ws.q4/q5),
misc (28.2, 42.3, 42.5).


# emit_janet — argv/arg/argc backend parity

The DD#60 argv trio (argc / arg N / argv splat / array snapshot) is
implemented for the **native** Cranelift backend and the **c** oracle
(`emit_c.rs` — sets the runtime globals via `_start`, calls
`sash_argv_get` / `sash_argv_snapshot`). The **janet** backend
(`emit_janet.rs`) has no argv handling at all (never did — argv tests
were failing under `--janet` before DD#60 too).

## Why it's a separate task

The janet backend runs the emitted Janet **script** via `janet
.out.janet`, not a static ELF linking `libsash_rt.a`. So it cannot call
`sash_argv_get` / `sash_argv_snapshot`. It needs a Janet-native argv
mechanism (`(dyn :args)` / `(os/args)`), mapping:
  - `$(argc)`     → length of the Janet args (+1 for the program-name slot
                     to match C argc)
  - `$(arg N)`    → `(get args N)` with 0-based / abort semantics
  - `ArgvIndex`   → same, for the for-in slice loop
  - `ArgvSnapshot`→ build a Janet table/array from the args slice

## Current state (2026-06-16)

argv.sh under `--janet`: ~16 runtime tests fail (the validator-gate tests
pass — validation is backend-independent). No regression: these tests
were never supported in janet. Full spec run shows janet at 123 failing
(baseline 104) — the +19 are entirely these new argv-feature tests, zero
janet regressions.

## See also
- `notes/decisions/DD60-argv-trio.md`
- `sash/src/emit_c.rs` (the c-backend argv impl to mirror)
- `sash/src/emit_janet.rs`
