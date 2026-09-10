# 183 — top-level `$@`/`$*` diagnostic: `$(argv)` → `$(argv 1)`

Small correctness fix to the SB5024 diagnostic text, following the `$@`
drop (journal 182).

## The imprecision

`$@`/`$*` are function-local (DD#46). At top level a reference is a
compile error (SB5024) that directs the author to the DD#60 argv trio.
The directive said:

    $@ is function-local — use $(argv) for iteration or $(arg N) at top level

But `$(argv)` is the **whole** array **including the program name at
index 0** (spec/argv.sh av.s1: "iterates the whole array from index 0
(includes program name)"). The function-local `$@`/`$*` are the
**user arguments** — which is `$(argv 1)`, the user-arg slice
(av.s2: "iterates the user-arg slice (index 1..)").

Directing `$@` → `$(argv)` would silently prepend the program name to
what the author expects to be their arguments. The correct target is
`$(argv 1)`. This matches DD#60, which was decided in favour of
`$(argv 1)` for the user-arg slice over a bare top-level `$@`.

## Change

`flag_positional` (validate.rs) now emits:

    $@ is function-local — use $(argv 1) for the user-arg slice
    (or $(arg N) for one) at top level

`$1`..`$N` → `$(arg N)` and `$#` → `$(argc)` are unchanged (both already
correct; `$(argc)` is C argc and the note already says to subtract 1 for
the user count).

DD46-positional-local.md updated to match, with the `$(argv)`-includes-
program-name caveat spelled out.

## Specs

spec/errors.sh e.dd46.t1 (`echo "$@"`) and e.dd46.t3 (`echo "$*"`) now
assert `use $(argv 1)`. e.dd46.t2 (`$1` → `$(arg 1)`) unchanged.

The rejection itself is unchanged — top-level `$@`/`$*` is still SB5024.
Only the suggested replacement text is corrected.

## Verification

Full `SPEC_MODES="native c janet"` sweep, latest-run-per-test diff vs
base `6fa9885`: 0 passing→failing, 0 gone, native gate 0 failing.
