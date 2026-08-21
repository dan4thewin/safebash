---
title: "docs: tail-recursion example + function/args/recursion drift (SUBSET/GUIDE)"
status: ready
priority: P3
kind: docs
depends: []
gates:
  - "docs show a runnable tail-recursion example (accumulator) and state the policy: only a direct self tail-call (`return F …`) is allowed; non-tail + mutual recursion → SB5037"
  - "SUBSET.md no longer lists `shift` as a builtin (dropped, DD#75 §5) and no longer lists `$*` (removed → `$(join args)`)"
  - "positional docs reflect the DD#75 §5 0-indexed implicit `args` array ($1 ≡ ${args[0]}, $@ ≡ @args, $# ≡ ${#args})"
  - "no doc-only test breakage (docs aren't linted, but keep examples compile-clean if pasted)"
---

# docs: tail recursion + function/args/recursion drift

Doc-only follow-up to the 2026-08-21 function work (DD#75 §5 args model, DD#27
recursion). DG: "get that tail-recursion example into a doc-update task and keep
after the backlog." The docs (esp. `docs/SUBSET.md`) drifted; fold these in as
one coherent pass.

## 1. Tail-recursion example (the primary ask)

Add a runnable example + the policy. Canonical accumulator form (verified,
native+c; janet uses its own recursive emit but same result):

```sash
fn fact -> int {
  int n=$(($1)); int acc=$(($2))
  if (( n <= 1 )); then return $(( acc )); fi
  int n1=$(( n - 1 )); int a1=$(( n * acc ))
  return $(( (fact n1 a1) ))     # tail self-call → compiled to a loop
}
int r=$(( (fact 5 1) ))          # 120
```

Policy to state (DD#27, amended):
- Functions are **inlined** (no runtime call stack), so recursion cannot be
  inlined in general.
- A **direct self tail-call** — the recursive call is the producer of a
  `return` (`return F …`) — is compiled to a **loop** (O(1) stack). This is the
  only allowed recursion.
- **Non-tail** self-recursion and **all mutual** recursion are rejected with
  **SB5037** (a clean compile error, not a crash).
- Since a nested `$(( ))` arg now parses (bug-arith-sexpr-nested-cmdsub-arg,
  fixed), `return $(( (go $(( n - 1 )) …) ))` also works, but the bind-a-local
  accumulator style above reads best.

## 2. `shift` removed (DD#75 §5)

`docs/SUBSET.md:219` still lists `shift` in the builtin set — remove it. `shift`
was dropped (a degenerate slice; a general array slice is post-beta,
`research-array-slice`). Iteration is `for x in @args`.

## 3. `$*` removed; the `args` model is 0-indexed (DD#75 §5)

- `docs/SUBSET.md:173` "Positional params: `$1`–`$9`, `$#`, `$@`, `$*`." — drop
  `$*` (removed → `$(join args)`).
- `docs/SUBSET.md:249` SB5024 row mentions `$*` — update.
- State the model: a function's arguments are the implicit **0-indexed** array
  `args`; `$1 ≡ ${args[0]}`, `$@ ≡ @args`, `$# ≡ ${#args}`. `$0` stays the
  program name (`$(arg 0)`), not part of `args`. (Cross-ref DD#75 §5, DD#60 for
  the `arg`/`argc`/`argv` process-argv trio — distinct from the function
  params.)

## Sources

- `notes/decisions/DD27-trampoline.md` (recursion amendment)
- `notes/decisions/DD75-canonical-keyword-forms-uniform-collections.md` §5
- specs: `functions.sh` 27.1–27.5, `types.sh` 75.17–75.22, 58.14/58.15
