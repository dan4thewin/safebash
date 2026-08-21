---
title: "DD#75 §5 args model — remaining polish (len/join/keys on args, $* removal, $@→@args for >9, recursion guard)"
status: ready
priority: P3
kind: task
depends: []
gates:
  - "$(len args) / $(join args) / $(keys args) resolve the implicit args array (currently fall through to external — `args` is not in the intrinsic-recognition collection set)"
  - "$* is removed → $(join args) (currently still pre-expands like $@); SB error directing to $(join args)"
  - "$@ with >9 args: switch the $@ pre-expansion to an @args splat so all args flow (today $@ pre-expands to $1..$argc and $10+ lower to empty — pre-existing cap, unchanged)"
  - "recursion via inlining currently stack-overflows the compiler — add a recursion guard (clean SB error, not a crash)"
  - "friendlier `shift` removal diagnostic (SB5011 → a 'use for x in @args' hint)"
---

# DD#75 §5 args model — remaining polish

The **core** args model shipped 2026-08-21 (unique-name per-invocation array;
`$1..$9`/`$@`/`$#` desugar; nested/recursive calls no longer leak positionals;
`shift` dropped; `${args[i]}`/`@args`/`${#args}`/`for x in @args` all work
native+c). This tracks the remainder.

## 1. `len`/`join`/`keys` on `args`

`$(len args)`/`$(join args)`/`$(keys args)` currently fall through to an
external command (`len: command not found`, GNU `join`, …). The intrinsic
lowering sites (`transform/expr_handlers.rs::try_len_intrinsic`,
`try_keys_intrinsic`, `join_intrinsic`) resolve the operand name but the
**recognition** of len/join/keys-as-intrinsic (vs external) checks
`collections.contains("args")` — and `args` is not a collection (the unique
`__argsN` is). Fix: apply `expressions::resolve_args_name` at the recognition
site too (wherever the @splat/collection requirement gates intrinsic-vs-external).
`$(len args)` ≡ `$#` and `$(join args)` is the `$*` replacement, so this closes §5.

## 2. `$*` removal

DD#75 §5 removes `$*` → `$(join args)`. Today `$*` still pre-expands to the
positional list (like `$@`) via `expand_dollar_at_words` (it treats `@` and `*`
alike). Make `$*` a compile error directing to `$(join args)`.

## 3. `$@` → `@args` splat (the >9 case)

`$@` currently pre-expands to `$1..$argc` (`expand_dollar_at_in_body`), and
`$10+` lower to empty (only `$1..$9` are single-digit sugar). So `$@` with >9
args drops the tail — a **pre-existing** cap (HEAD had it too). Switch the `$@`
pre-expansion to an `@args` splat (recognize `SimpleExpansion("@")` in
`arg_to_value` → `ArraySplat{current_args_name}`) so all args flow regardless of
count. Remove `expand_dollar_at_*` once `$@`/`$*` are handled at lowering.

## 4. Recursion guard

A recursive function inlines infinitely and **stack-overflows the compiler**
(`down(){ …; down $((n-1)); }`). Pre-existing (HEAD too — no recursion
detection). Add a guard: detect a function inlining itself (a call-stack set in
`VersionState`) → clean SB error ("recursion is not supported"), not a crash.
Independent of the args model.

## 5. Friendlier `shift` diagnostic

`shift` now resolves as an unknown external → SB5011. A dedicated "shift is
removed — use `for x in @args` (or a future array slice)" message would be
friendlier. Nice-to-have.
