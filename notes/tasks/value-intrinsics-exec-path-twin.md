---
title: "value intrinsics with a PATH twin silently exec it in statement position"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`basename /a/b`, `dirname /a/b`, and `join a \",\"` in statement position are a compile error (SB5073), not a silent fork of the coreutils program"
  - "the same holds for every value intrinsic regardless of what is installed on the build host — `len`, `keys`, `toupper`, `tolower`, `strlen`, `sub`, `gsub`, `random` included"
  - "a spec pins at least one name that has a real PATH twin and one that does not, so the invariant cannot regress into being host-dependent again"
---

## Symptom

Measured 2026-09-13 against `target/release/sash`:

| intrinsic | in `BUILTINS` | PATH twin | statement position |
|---|---|---|---|
| `len`, `keys`, `toupper`, `strlen`, `sub`, `gsub`, `random` | no | none | SB5011 |
| `join` | no | `/usr/bin/join` | **silently execs it** |
| `basename` | no | `/usr/bin/basename` | **silently execs it** |
| `dirname` | no | `/usr/bin/dirname` | **silently execs it** |

```
$ cat bn.sh
basename /a/b
$ sash --emit-c bn.sh
    const char* __t0_argv[] = { "/usr/bin/basename", "/a/b", (const char*)0 };
    __exit_status = sash_run((const char* const*)__t0_argv, sash_envp);
```

Not backend-specific: `resolve::resolve_commands` runs at `lib.rs:168`, ahead
of codegen, so native does exactly the same.

## The invariant it violates

`sash/src/validate.rs`, the doc comment on `VALUE_ONLY_INTRINSICS`, states the
rule and then names its own counter-example:

> They stay in `BUILTINS` above so they are never PATH-resolved (a machine with
> a real `getenv` on PATH must not silently exec it) […] The other value
> intrinsics (`len`, `keys`, **`join`**, `toupper`, `random`, …) are not in
> `BUILTINS` at all, so they already get SB5011 in statement position.

`getenv` is protected by membership in `BUILTINS`. The rest were assumed safe
because no PATH twin exists — an accident of naming that is false for three of
them, one of which the comment lists as an example of the safe case.

## Why P1

The hazard is the one the comment was written to prevent: the compiler silently
executes a program off the build host's PATH where the user wrote an intrinsic.

It also makes the **compile result host-dependent**. The same source is SB5011
on a minimal image and a fork on a coreutils image. For a compiler whose pitch
is dm-verity / hermetic execution (DD#23: "no PATH injection, no symlink tricks,
no search ambiguity"), what the build machine happened to have installed must
not be an input to the output.

## Decision (DG, 2026-09-13)

**Scope B — all value intrinsics**, not just the three that have a twin today.
Add `len`, `keys`, `join`, `toupper`, `tolower`, `strlen`, `sub`, `gsub`,
`basename`, `dirname`, `random` to `BUILTINS` *and* `VALUE_ONLY_INTRINSICS`, so
statement position is SB5073 and the name is never PATH-resolved.

Rationale: this is the stated principle applied to all of its consequences
rather than to the subset that happens to be broken today. It also makes the
diagnostic uniform — SB5073 ("value-producing intrinsic, legal only as a `$( )`
head") is a better message than SB5011 ("cannot resolve command") for every one
of these names, including the ones that already error.

## Blast radius

Zero. No spec uses any of them in statement position, and `sash-convert`
already routes bash's `basename`/`dirname` through
`sash-convert/src/known_externals.rs` to absolute paths, so conversion output
is unaffected.

## Notes

Found while auditing the `_ =>` fail-open arms in `emit_c.rs`
(`fail-open-todo-arms-in-emit-c`). A reachability probe of every `BuiltinCmd`
in command position showed `basename` and `join` compiling when every sibling
intrinsic was rejected; DG asked why the c backend wanted an external, which is
what turned it up. The probe was looking for a c-backend hole and found a
front-end one.

---

## DONE 2026-09-13 (journal 282)

Scope B as DG directed. All 11 value intrinsics — `len`, `strlen`, `keys`,
`join`, `sub`, `gsub`, `toupper`, `tolower`, `basename`, `dirname`, `random` —
added to **both** `BUILTINS` (so they are never PATH-resolved) and
`VALUE_ONLY_INTRINSICS` (so statement position is SB5073). The doc comment
that asserted the false invariant was corrected in place.

`random` has no `BuiltinCmd` variant and never appears in `from_name`; it is
resolved by name in `transform/expr_handlers.rs`, which is why a search of the
builtin table missed it.

Specs `via.13-20` in errors.sh, deliberately split: `basename`/`dirname`/`join`
have a real PATH twin on this host, `random`/`len`/`toupper` do not. The second
group is the one that makes the property host-independent — it must reject for
being value-only, not because a PATH lookup happened to miss. `via.19-20` pin
that the value-producing forms still work.

Fail-first: reverting validate.rs fails exactly via.13-18 in both modes.
