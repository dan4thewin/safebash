title: "implement getpid-class internal builtins (getpid/getppid/getcwd/epochseconds/random/argc); $$ = sugar for getpid"
status: done
priority: P3
kind: feature
depends: []
gates:
  - "$(getpid), $(getppid), $(getcwd), $(epochseconds), $(random), $(argc) work as in-process internal builtins (no execve) and capture CLEAN"
  - "$$ is sugar for getpid (same value, clean)"
  - "getcwd is a builtin (not the external /usr/bin/pwd, which would be tainted)"
  - "native + c + janet; spec tests; no net regression"

# Implement the getpid-class internal builtins

> **DONE (2026-07-07, autopilot) for native + c.** The two missing arms
> landed: `getpid` (native `value_handlers` arm — c already had it) and
> `random` (new runtime `sash_random` + native/c arms). `$(random)` returns
> a **non-negative 63-bit i64 (0..i64::MAX)** from `getrandom(2)` (rustix
> `rand` feature; same CSPRNG as /dev/urandom), sign bit cleared — DG's
> range decision. All getpid-class intrinsics
> (echo/getcwd/getpid/getppid/epochseconds/random/argc + `$$`) are clean,
> in-process, no exec. printf/pwd were moved OUT to external this session
> (commit d3a4072). spec/functions.sh 42.7 (getpid numeric + clean
> capture), 42.8 (random non-negative), 42.9 (two draws differ), guarded
> native,c. 0 regressions.
>
> **janet is OUT of scope here (filed `janet-getpid-class-intrinsics`):**
> janet emits `(os/spawn ["getppid"])` etc. for the WHOLE class — it tries
> to exec the intrinsics as external commands. That is a pre-existing janet
> backend gap for every getpid-class name (getenv/argc too), not specific
> to getpid/random; it belongs to the c/janet parity tier.

## Verified state (2026-07-06) — PARTIAL

Behavioral check (native, release binary), one builtin per script:

| builtin           | status | note |
|-------------------|:------:|------|
| `$(getppid)`      |   ✓    | value_handlers dispatch arm + `sash_getppid` |
| `$(getcwd)`       |   ✓    | dispatch arm + `sash_getcwd` |
| `$(epochseconds)` |   ✓    | dispatch arm + `sash_epochseconds` |
| `$(argc)`         |   ✓    | dispatch arm |
| `$$`              |   ✓    | wired as its own special-var path |
| `$(getpid)`       |   ✗    | **compiles, then runtime-aborts** exit 127: `bug: bare command passed to sash_exec` |
| `$(random)`       |   ✗    | same runtime abort |

Root cause of the two failures: `getpid` and `random` are in
`sash/src/resolve.rs`'s intrinsic-skip list (and the cmdsub clean-list),
so resolution does **not** turn them into external commands — but
`sash/src/codegen/value_handlers.rs`'s cmdsub-intrinsic dispatch has arms
for `epochseconds` / `getppid` / `getcwd` / `getenv` / `argc` / `signum`
and **no `getpid` / `random` arm**. So they emit as a bare command name
and hit the DD#23 `sash_exec` guard at runtime. An advertised-clean
builtin that runtime-aborts is a sharp edge.

Remaining work:
1. Add a `getpid` dispatch arm (runtime `sash_getpid` already exists —
   `$$` uses it).
2. Add a `random` arm — needs a runtime `sash_random` **and a semantics
   decision** (bash `$RANDOM` is 0..32767; range / seeding for sash?).
3. Defensive alternative worth considering: until a name is lowered, drop
   it from the intrinsic-skip / clean lists so `$(name)` gets a clean
   `SB5011` at compile instead of a runtime `bug:` abort.
4. c / janet parity + spec tests.

## Original intent

These names sit in `cmd_subst_taint`'s clean list; `getpid` / `random`
are not yet lowered (above). DG: build them as real **internal builtins**
(in-process, no execve → clean), and make **`$$` sugar for `getpid`**.
`getcwd` is the builtin form (use it instead of the external `pwd`, which
execs → tainted).

Runtime support likely needs `sash_getpid`/`sash_getppid`/`sash_getcwd`/
… (rustix syscalls); wire each into the cmdsub-capture / intrinsic path
(where getenv/argv/arg are handled) and the direct-command path.
