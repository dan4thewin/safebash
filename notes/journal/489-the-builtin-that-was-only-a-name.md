---
date: 2026-10-10
---

# The builtin that was only a name

Working the coverage queue after `9c2e35a`, the ranking put
`resolve::resolve_command` near the top with nine cold lines. Two of them were
already understood — the non-`Literal` head guard carries a long comment
explaining that `validate` rejects those first. The other seven were an
`SB5011` "command not found" push whose message is **character-for-character
identical** to the one `validate.rs` emits. A second copy of a diagnostic is
the shape that has bitten this project twice before, so the question was
whether anything could reach it.

## Nothing should have been able to reach it

`resolve_command` only consults the PATH when `is_resolvable_external` says
yes, and that is `!name.contains('/') && !is_intrinsic(name) &&
!funcs.contains(name)`. `validate`, which runs first and exits on failure,
only lets a bare name through when it is in `BUILTINS`, is a declared
function, or satisfies `resolve_on_path`. And `resolve_on_path` calls *the
very same* `resolve_command_path`. So a name that got past `validate` by the
PATH route returns `Some` here by construction, and the other two routes
return early. The arm should be dead.

Unless `BUILTINS` and `is_intrinsic` disagree about what a builtin is. They
are two hand-maintained string lists, and nothing relates them.

They disagree about exactly one name.

```
$ comm -23 validate_BUILTINS ssa_BuiltinCmd::from_name
arg argc epochseconds getcpid getcwd getenv getpid getppid
kill random signum status waitstatus
```

Twelve of those thirteen are in `resolve::INTRINSICS`, which `is_intrinsic`
also consults. The thirteenth is `kill`.

## What `kill` actually was

`"kill"` occurs **once** in the whole of `sash/src`: the `BUILTINS` entry, filed
under "Process/signal" between `wait` and `signum`. There is no
`BuiltinCmd::Kill`, no `INTRINSICS` row, no `type_check` entry, no codegen, no
runtime. It was a name in a list and nothing else, apparently since the list
was written.

The consequences were not academic.

```
$ printf 'kill 0\n' > k.sh && sash --manifest k.sh
/usr/bin/kill
```

Because `is_intrinsic("kill")` was false, `resolve_command` rewrote the head
to the external binary. `kill 0` was a fork/exec of coreutils all along.

That is a *host-dependent* safe set, which is precisely the bug the comment
above the list already describes someone fixing once:

> Value-only intrinsics. They are listed here for exactly one reason: a name
> in BUILTINS is never PATH-resolved. Leaving them out made the safe set
> host-dependent — `join`, `basename` and `dirname` have coreutils twins and
> silently exec'd them in statement position, while the rest escaped only
> because no binary of that name happened to be installed.

The remedy recorded there was "add them to `BUILTINS`". But that remedy does
not work, because **`BUILTINS` membership is not what suppresses PATH
resolution** — `is_intrinsic` is. It worked for `join`, `basename` and
`dirname` only because those names are *also* `BuiltinCmd`s. The comment states
an invariant the code never enforced, and `kill` is the case where the
coincidence ran out.

## The part that mattered

`BUILTINS` has a second reader, and it is the taint pass:

```rust
/// Check if a pipeline stage is an external source (execs). Shared with the
/// transform's SSA-backstop `clean_pipe` computation so the two taint layers
/// classify a stage identically — a divergence here is a laundering hole.
pub(crate) fn is_external_source(stmt: &Statement) -> bool {
    …
    if !name.is_empty() && !BUILTINS.contains(&name) {
        return true;
    }
```

So `kill` exec'd a real external while this function reported it
non-external. Its stdout was treated as data from another sash stage —
clean. A laundering hole, in the function whose own doc comment names that
exact failure.

The A/B is one character of program text apart:

```
echo hi | read x    →  no SB5020   (echo is in BUILTINS: clean)
kill -l | read x    →  no SB5020   (kill was in BUILTINS: clean — but it execs)
```

## The fix

DG: "a; prefer parsimony — `kill` is a low volume operation." So `kill` comes
out of `BUILTINS` and becomes an ordinary external. On a host that has it,
nothing changes: `validate`'s `resolve_on_path` blesses it, `resolve` rewrites
it to `/usr/bin/kill`, the manifest is the same. On a host that does not, the
same SB5011 text now comes from `validate` instead of `resolve`. And the pipe
is correctly tainted:

```
kill -l | read x    →  SB5020 tainted variable $x used without attest
```

## The guard

Removing the entry fixes today. The *shape* of the bug is two hand-maintained
lists with a stated subset relation and no enforcement, so the entry could
come back tomorrow. Two unit tests now hold the claims the comments make:

- `every_builtin_name_is_an_intrinsic` — every `BUILTINS` name is owned by
  `resolve::is_intrinsic`. This is the one that was false.
- `value_only_intrinsics_are_all_builtins` — the companion claim from
  `VALUE_ONLY_INTRINSICS`' doc comment.

Proven capable before being trusted: adding a stray name fails the first with
`BUILTINS entries resolve::is_intrinsic does not own … ["zzz_prove_the_gate"]`.

## What happened to the cold lines

They are still cold, and now unreachable by construction — the subset relation
the unit test enforces is exactly what closes the last route to them. They stay
a diagnostic rather than becoming `unreachable!()`, which is a deliberate call
against this repo's usual "unreachable by construction → ICE" rule: this arm is
the graceful backstop for *precisely* the drift that `kill` was, and it asks an
orthogonal question to the unit test's. A crash would be a worse answer than a
correct error message. The comment now says so.

So the coverage queue's verdict on `resolve_command` is category 2 for all nine
lines — but only *after* the fix. Before it, line 455 was reachable, by a name
nobody had noticed was a lie.

## Also in this batch

The sysroot work from the previous session closed out: `squashfs_listing` was
restructured from a lazy populating function into a cache reader that asserts
its invariant (`check_sysroot` primes the cache in `check_flags`, before
`read_source`, so the lazy re-read was unreachable), with a `catch_unwind` unit
test. Three specs — `cli.40` (sysroot is not a directory), `cli.41`
(`unsquashfs` not installed), `cli.42` (present-but-corrupt image) — cover the
two genuine gaps plus the hazard case.

## Gates

Sweep **8085/8085**, predicted exactly (8079 + 3 specs × 2 modes); zero
regressions; machinery proven capable at 248 flips; six new ids, all expected.
`mk test` green, clippy clean, totality clean.

## A note on method

The cold line was the hypothesis; `kill` was the finding. What made the
difference was refusing to accept "`write hi` prints `hi`, so builtins must be
filtered somewhere" as an answer. There was no filter — `write` is a
`BuiltinCmd`, so it never reached the PATH search, and the one name that did
was the one I had not thought to test.
