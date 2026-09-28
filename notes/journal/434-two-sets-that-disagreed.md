# 434 — two sets that disagreed, and a subshell that was never there

This session started with DG asking a model question and ended with a P0 fixed,
a language rule tightened, and a piece of the compiler deleted. The question
was:

> sash doesn't have subshells - so changes to globals, for example, survive
> unless a fork is required. […] I would have expected something logically
> equivalent to a recursive compilation of `$(str h=$(getenv)...)` which should
> catch the taint using the same mechanisms as the top layer. What happens
> instead?

Answering it honestly meant reading what actually happens, and what actually
happened was not what anyone had written down.

## What happens instead

Codegen *does* recursively lower a `$( )` body. It goes inline into the same
SSA function — `_blk_1: /* cap_body */` — with no fork when the body is pure.
Validate is the layer that does not recurse: positionals (SB5024), elide
(SB5046), argv-splat (SB5025), resolve and type-check all reach the body, and
taint substitutes a four-line walker that only looks at `StmtKind::Command`.

So DG's expectation was right about the mechanism and the gap was in one pass.
That is the still-open `bug-taint-escape-in-cmdsub-body`.

But reading the emitted C to confirm the recursion turned up something else:

```c
const char* __save_bodycap_x_ptr = x_ptr;
...
x_ptr = __save_bodycap_x_ptr;
```

An explicit snapshot/restore, synthesised around every capture body, emulating
a subshell that this language does not have.

## The snapshot was wrong three ways

I was ready to call it a deliberate bash-compatibility choice. It was not, and
the evidence is unambiguous:

1. **It contradicted DD#86.** A global is read *and written* with no friction
   from any scope. Nothing in the model made a capture body a scope boundary.

2. **It contradicted itself.** Measured before touching anything:

   ```sh
   str x=one; fn f { x=two; echo hi }; str s=$(f)   # x is two
   str x=one; str s=$(x=two; echo hi)               # x is one
   ```

   Same effect, two spellings, two answers. This is the tell that matters —
   not "which one is right" but that the compiler did not know.

3. **The save set was not total.** `collect_assigned_into` handled only
   `Assignment` and `Declaration`. A subscript write escaped the restore. So
   did a `read`. An isolation you cannot rely on is worse than no isolation,
   because code gets written against it.

DG's ruling: *"a `$( )` capture could be modeled as an anonymous function; all
new variables would be local."* Five functions deleted, the doc comment
rewritten, two specs rewritten to assert the new behaviour rather than silenced.

## Chasing the third point found a P0

Point 3 was a hypothesis, so I went to confirm it — `read x` inside a body, does
it clobber the outer `x`? The probe printed garbage. `od` said the garbage was
`0x7f…`.

Minimal repro, no command substitution anywhere:

```sh
argv x
attest e in x [[ -n $e ]]
/bin/echo $x
```

A live mmap address, handed to an external process, fresh every run. Both
backends identically. `--emit-c` shows the whole thing in four lines: the
`SashTable*` goes into the **scalar** pointer slot with `x_len = 0`,
`sash_argv_guard` is handed length 0 and so validates nothing, and `execve`
reads that pointer to the first NUL — through the table header, whose first
field is itself a pointer.

### Two sets that disagreed

`read`, `argv` and the 3-argument `match`/`matchall` auto-declare a collection.
`type_check.rs` knew that and registered them in `env.collections`.
`validate.rs::collect_collection_names` built its own set from explicit
`array`/`map` declarations only — and SB5064, the guard written for exactly
this spelling, consulted the incomplete one.

```
array x=(a); /bin/echo $x   →  SB5064, correctly
argv x;  …;  /bin/echo $x   →  compiles, leaks
```

The second fault sat right behind it: auto-declare inserted unconditionally,
so `str x=safe; read x` silently retyped the name, and `int x=1; read x` was a
compiler ICE.

### Why no spec found it

Every accidental masker fires first. SB5020 blocks the untainted spelling;
SB5050 blocks the short forms; SB4018 blocks the `matchall` one. The leak is
reachable only *after* you satisfy the guards that were hiding it — which is
why it took a session about something else to walk into it.

That is the third time this pattern has cost us. It is worth stating plainly:
**a guard that only fires after other guards are satisfied has never been
tested by anything that stops at the first diagnostic.**

## The fix, and one thing the ordering forced

DG settled both faults in one line: *"no shadowing and no rebinding. `$x`
should be an error after `argv x` (SB5064?)."*

The filing had asked where the single source of truth should live, since
`type_check` already computes this set. It cannot be shared: **validate runs
before type_check**, and a validate diagnostic aborts. The set has to be
computable from the AST alone.

That same ordering produced the one subtle part. A first cut made
`x=safe; /bin/echo $x; read x` report SB5064 against **line 2** — a line that
is entirely correct — because by then `x` was in the auto-collection set. The
user would never see the real fault on line 3. So validate subtracts scalar
bindings and declines the case, letting type_check own the better message. An
explicit `array`/`map` declaration deliberately still wins.

Fault 2 merged `register_argv_target` and `register_newread_target` into one
`register_collection_target` with a DD#86 conflict check. The ICE is gone by
the same route.

## Numbers

Nineteen new specs — `types.sh` 75.101–75.107, `scope.sh` 86.59–86.70 — and
two rewritten (`commands.sh` 14.3b, `errors.sh` csc.7), both of which had
asserted the subshell that was never there.

Predicted sweep total 6721 before running it; got 6721. **6708 passing, 13
skipped, zero failing.** `mk test` green, clippy and fmt clean.

## Still open

- `bug-taint-escape-in-cmdsub-body` — the P1 that started all this. Note the
  anonymous-function model *changes the fix*: the old framing was "clone the
  tainted set for the body", which was subshell thinking. A body write to a
  global now persists, so taint must persist out of a body too.
- **SB5050 does not reach a body either.** `$(int x=1; echo hi)` compiles with
  a dead variable. Same theme, not filed yet.
- `collect_bulk_targets` wraps `collect_read_targets`, which includes
  `readline` — a *scalar* binder. `check_collection_args` therefore
  over-collects it. Pre-existing, latent, not investigated.
