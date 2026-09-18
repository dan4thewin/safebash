# 359 — A feature with no specs, and state with no readers

Continued down `check_len_operand` after the purge in journal 358. Three
cold branches were left. One is the irreducible `Option` arm recorded
last time. The other two each turned out to be a different kind of
finding, and neither was what the code looked like it was saying.

## `len args` — working, correct, and completely untested

The guard ends `|| name == "args"`, and the *true* outcome never fired
in 5480 specs. The obvious reading is that the arm is dead, as five of
its neighbours had just proved to be. It is not:

```
fn f { echo "$(len args)" }
f a b c          →  3
```

`args` is the function-local argv collection — DD#75 §5, registered by
`check_function`, referenced at eighteen sites in `validate.rs`. `len
args` is a legitimate spelling and it works. The branch was cold because
**no spec had ever run it**, which is a different problem from dead code
and wants the opposite fix.

Probed it properly before writing anything down. It agrees with `$#`,
counts empty-string arguments rather than skipping them, reports 0 for a
call with no arguments rather than erroring, is per-frame across a
nested call, and `keys args` correctly refuses it as an indexed array
(SB4030). No bug — but a feature whose entire behaviour rested on
nothing.

The near-miss is worth recording: `join args` *is* covered (cf.f11), and
`grep args spec/*.sh` returns hits in twenty-two files, so the area
looks well covered from any distance. The only mentions of `len args`
in the suite are two **convert** specs asserting the converter does
*not* emit it (57.129, 57.130 prefer `$#`). A negative assertion about a
spelling reads, at a glance, exactly like coverage of that spelling.

Seven specs added, `75.94`–`75.100` in `types.sh`, one per property
probed above.

## `loop_vars` — a field whose only reader could not decide

The other cold branch was `env.loop_vars.contains(&name)`. Probing said
`len x` on a loop variable *does* error, SB4030, in every loop form —
word list, `@a` splat, slot splat, C-style header. So the guard works;
the disjunct simply never gets asked, because `env.vars` short-circuits
first.

It short-circuits because of a change I made three commits ago.
`check_for_binding_type` either finds the loop variable already in
`vars` or declares it there — either way it is in `vars` before the body
is checked. That is the second time in two journals that a branch went
cold because we fixed the thing that used to reach it.

Asking the index what else read the field gave the real shape:

```
type_check.rs:39    loop_vars: HashSet<String>      declaration
type_check.rs:64    loop_vars: HashSet::new()       init
type_check.rs:1500  env.loop_vars.insert(…)         the only writer
type_check.rs:1065  env.loop_vars.contains(&name)   the only reader
```

One writer, one reader, and the reader can no longer decide anything.
The whole field is dead state — a `HashSet` allocated per `TypeEnv` and
inserted into once per `for` loop, to answer a question `vars` had
already answered. Purged entire.

### The comment that made this worth pausing over

The field carried a doc comment giving a deliberate rationale:

> `for v in …` variables. An element of a collection is always a scalar,
> but the inference pass has no `For` arm, so these never reach `vars`.
> Tracked separately rather than declared, so that adding them cannot
> perturb the string/int rules for a name the pass deliberately leaves
> untyped.

"Tracked separately rather than declared" is exactly the thing I had
undone. The house rule is to stop when a change collides with something
that looks deliberate, and this looked deliberate.

It is superseded, and by DG rather than by me — from earlier in this
same session:

> DG: *"a for variable survives the loop, making the for an implicit
> declaration, rather than a non-declared special var."*

A for variable is a declaration; the premise "a name the pass
deliberately leaves untyped" no longer describes the language. Checked
provenance this time rather than assuming, which is the correction from
journal 356 applied in the direction it was meant to run — it cuts
against my convenience here as easily as for it.

One thing the comment says remains true and is worth not breaking:
`infer_types` builds a **fresh** `TypeEnv`, so the inference pass still
has no `For` arm and still leaves loop variables untyped in the map
codegen reads. Nothing here changed that. The sweep agrees.

`check_len_operand` is now cc 5 / cog 3, down from 8/4 before journal
358 touched it.

Sweep 5480 → **5494**, zero failing — +7 specs in a dual-mode file,
+14, the total predicted before running it. Units 92.

## Aside: what the function metric is actually measuring

Regenerated coverage first, since it predated SB5080/5081/5082. Fresh
numbers: **lines 96.85%, functions 98.28%, branches 89.91%** (375 cold).

The 56 "uncovered functions" do not mean 56 dead functions. Broken down:

- **~22 are cold monomorphizations** of eight heavily-used generic
  walkers in `ir.rs`. `direct_exprs` has 38 instantiations, 434,553
  hits, and one cold — a single call site whose closure type never ran.
- **~32 are cold closures** nested inside live functions, reported by
  lcov under the enclosing function's name.
- **4 are genuinely never called**, and three of those are ICE helpers
  (`panic_payload_message`, `redirect_target_ice`,
  `ice_unreachable_command`) that are cold *by design*.
- The fourth, `opaque_form_to_string`, is covered by the unit test
  `every_expr_kind_renders` — which `mk cov` deliberately excludes.

So the function metric is at its practical ceiling under specs-only
measurement, and the residual is mostly the line/branch queue wearing a
different hat. Reaching a literal 100% would require either counting
unit tests (`mk cov-unit`, off by design because it hides
should-never-run assertions) or deleting the ICE guards. Branches at
89.91% are where the remaining signal is.
