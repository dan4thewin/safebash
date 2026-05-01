# Design Decision #46: Positional parameters are function-local only

### Bug that surfaced this

Empirical test (2026-04-24) of:
```bash
foo() { echo "got: $2"; }
foo onearg
```
compiled with sash and run with program args `first second third`:
- bash output: `got: ` (empty)
- sash compiled output: `got: second`

Program argv leaked through unbound positional slots inside the
function. Root cause: `transform.rs` inliner only redefines `$1..$N`
for the args supplied at the call site; `$N+1..$9` retain whatever
SSA version was reaching from the caller, which at top level is
version 0 loaded from `sash_argv`. Both a bash-divergence bug and a
structural enabler of taint bypasses under DD#24 amendment 2.

### The decision

**Sash-native mode**: `$1..$N`, `$@`, `$*`, `$#` are *only* valid
inside a function body. Their meaning is "argument bound by the
nearest enclosing call site." At top level, references to these
names are a compile error with guidance to use `argv N` /
`argv` (the explicit boundary builtin per DD#42).

**Bash mode** (compiling unconverted bash scripts): top-level
`$1..$N` are coerced to `$(argv N)`, top-level `$@` / `$*` to
`$(argv)`, top-level `$#` to argc-1 via the appropriate builtin.
The coercion is visible via `--emit-source`, like other bash→sash
transforms.

**Both modes**: inside a function body, reading `$K` where `K`
exceeds the call's actual arg count (`$#`) is a compile error,
not silent empty. Bash returns empty for unbound positionals; sash
treats it as a programmer mistake and rejects at compile time.
This requires the inliner to know the per-call argc statically,
which it already does (the call-site `c.args.len()`).

### Why

DD#24 amendment 2 enumerates four taint sources, one of which is
"program positional params." Having `$1..$N` *also* mean "function
arg" inside a body creates two meanings for one syntax, with the
disambiguation depending on lexical context. Removing the top-level
meaning makes:

- Taint classification syntactic: `argv` and `getenv` are the only
  ways to introduce program-level external data; positional-param
  syntax in any position means "function arg," whose taint is the
  taint of the call expression (typically clean).
- The leak structurally impossible: at top level there is nothing to
  leak from; inside a function the inliner's bindings fully define
  the slots and out-of-range reads are rejected.
- `$(argv N)` becomes the canonical, documented way to read program
  argv, matching the documented DD#42 idiom.
- Bash compatibility preserved via coercion, not via shared meaning.

### Implementation plan

1. Add a validator rule (CST layer) for sash-native mode: top-level
   reference to `$1..$9`, `$@`, `$*`, `$#` → diagnostic with
   suggested `argv N` / `argv` / `len argv` replacement.
2. Add a coercion (CST→IR or lower.rs) for bash mode: rewrite
   top-level positional refs to the corresponding `argv` calls.
3. In the inliner (`transform.rs:579-617` and the capture path
   `:147-220`): after defining `$1..$N` from call args, emit a
   diagnostic-bearing marker for slots `$N+1..$9` so any read of
   them in the body emits an "out-of-range positional parameter"
   error. (Or: track per-function the call-site argc, and have the
   use-site SSA reference check against it.)
4. Tests: add the leak test from above as a regression; add tests
   for top-level `$1` rejection in sash-native mode and coercion in
   bash mode; add tests for out-of-range reads in both modes.

### Lesson

The earlier fix to walk function bodies in the CST taint analyzer
was correct as far as it went, but the right architecture was
upstream: don't have positional-param syntax mean two things.
DG's question — "if a function takes one arg but reads $2, does
it get argv 2?" — exposed both a concrete bug and a design
clarification that reduces taint-pass surface area.

---

