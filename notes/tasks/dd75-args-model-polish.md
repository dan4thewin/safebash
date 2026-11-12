title: "DD#75 §5 args model — remaining polish (len/join/keys on args, $* removal, $@→@args for >9, recursion guard)"
status: closed
closed: 2026-09-12
priority: P3
kind: task
depends: []
gates:
  - "$(len args) / $(join args) / $(keys args) resolve the implicit args array (currently fall through to external — `args` is not in the intrinsic-recognition collection set)"
  - "$* is removed → $(join args) (currently still pre-expands like $@); SB error directing to $(join args)"
  - "$@ with >9 args: switch the $@ pre-expansion to an @args splat so all args flow (today $@ pre-expands to $1..$argc and $10+ lower to empty — pre-existing cap, unchanged)"
  - "recursion via inlining currently stack-overflows the compiler — add a recursion guard (clean SB error, not a crash)"
  - "friendlier `shift` removal diagnostic (SB5011 → a 'use for x in @args' hint)"

# DD#75 §5 args model — remaining polish

> **CLOSED (2026-09-12).** Items 1–4 are done (verified by probe, table
> below). Item 5 is superseded: DG ruled the removed-builtin keyword table
> goes away entirely, which makes SB5011 the uniform unknown-command answer,
> so `shift` needs no special case. See
> `notes/tasks/drop-removed-builtin-table.md`.

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

## Progress (2026-08-21)

- **`$(len/join/keys args)` resolved** — `resolve_args_name` applied at the three
  intrinsic lowering sites (`transform/expr_handlers.rs`), and `type_check`
  `check_function` registers `args` as a collection for the function-body scope.
  `$(len args)` = arg count, `$(join args SEP)` joins, `$(keys args)` correctly
  rejects (indexed). Fixed a silent-0 (`$(len args)` was 0). Specs types.sh
  75.23/75.24. native,c (janet's `_args` model doesn't expose `args`).
- **top-level `args` reject** — `@args`/`${args[i]}`/`${#args}` outside a function
  → SB5024 (function-local, like `$1`), via `flag_args_toplevel` in
  `validate.rs::check_positionals`. Specs 75.25/75.26.

## Verified 2026-09-12 — 4 of 5 items are done

Probed against the release binary, not read off the filing:

| item | state | evidence |
|---|---|---|
| 1. `len`/`join`/`keys` on `args` | **done** | `f(){ echo $(len args); echo $(join args ,); }; f a b c` → `3` / `a,b,c` |
| 2. `$*` removal | **done** | `$*` → SB5045 "sash has no `$*` — use `@args` … or `$(join args)`" |
| 3. `$@` → `@args` splat (>9) | **done** | `$@` is now rejected outright (SB5045); `for x in @args` with 12 args emits all 12. The ">9 drops the tail" cap the filing described is gone with the construct. |
| 4. recursion guard | **done, better than asked** | SB5037, a clean diagnostic rather than a compiler stack overflow — and it documents that a direct self tail-call is supported. `return down $(( … ))` compiles and runs. |
| 5. friendlier `shift` diagnostic | **open** | `shift` still gives generic SB5011 "command 'shift' not found — use absolute path or define as function" |

## Item 5 is a fork, not a one-liner

There is an existing removed-builtin table at
`sash/src/parse/parser.rs:1722–1746` (`source`/`.`, `export`,
`declare`/`typeset`, `readonly`, `select`). Adding `shift` there is four
lines — but it **reserves the name**, and that is a behaviour change:

    $ cat a.sash
    shift() { echo "my shift"; }
    shift
    $ sash a.sash && ./a.out
    my shift                      # works today

    $ cat b.sash
    select() { echo "my select"; }
    select
    $ sash b.sash
    b.sash:2:1: error [SB5001] sash has no `select`     # name is reserved

So the two options are:

- **(a) parser table**, like `select` — consistent with the rest of the
  removed-bash-builtin family, but a user-defined `shift()` stops working.
- **(b) special-case the message at command resolution**, where SB5011 fires
  — purely additive, better wording when `shift` is undefined, and a
  user-defined `shift()` still wins. This is what the filing's own wording
  ("SB5011 → a hint") envisioned, and it does not contradict SB5011's own
  advice to "define as function".

DD#75 §5 drops `shift` as a *builtin*; nothing in it reserves the *name*.

## Item 5 resolution (DG, 2026-09-12)

Neither option. DG: *"drop the removed-builtin table altogether. error just
as for any other unknown token in cmd position."* `shift` already gets
SB5011; once `export`/`declare`/`typeset`/`readonly`/`select` join it, the
generic message **is** the answer and no per-word hint is wanted. Tracked in
`notes/tasks/drop-removed-builtin-table.md`.
