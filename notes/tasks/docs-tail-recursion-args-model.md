title: "docs: tail-recursion example + function/args/recursion drift (SUBSET/GUIDE)"
status: done
priority: P3
kind: docs
depends: []
gates:
  - "docs show a runnable tail-recursion example (accumulator) and state the policy: only a direct self tail-call (`return F …`) is allowed; non-tail + mutual recursion → SB5037"
  - "MOOT: SUBSET.md was removed 2026-08-29 — it had drifted from the DDs. Re-measured: GUIDE.md already lists `shift` correctly (as an unimplemented builtin getting SB5011), and neither surviving doc mentions `$*`"
  - "MOOT as written — `$# ≡ ${#args}` is wrong twice over (see below). GUIDE.md and sash.1 now carry the recursion model; the args model is covered by the DD#46/DD#75 corrections made in journal 300"
  - "no doc-only test breakage (docs aren't linted, but keep examples compile-clean if pasted)"

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


---

## Outcome (2026-09-14)

Two of the four gates were **moot on arrival**, and one of those was moot in
a way worth recording rather than deleting.

### Gate 2 — `SUBSET.md` does not exist

Removed 2026-08-29 for having drifted from the DDs, which is the same failure
this task was filed to fix, one level up. Re-measured against the surviving
docs: `GUIDE.md:217` already lists `shift` correctly — as a bash builtin sash
does not implement, getting SB5011 like any unknown name — and neither
`GUIDE.md` nor `sash.1` mentions `$*` at all. Nothing to do.

### Gate 3 — `$# ≡ ${#args}` is wrong twice over

`${#args}` is SB4038 (DD#75 §4 retired the sugar), and the replacement is not
`$(len args)` either: DD#46 keeps `$#` function-local **and sash-native**.
This is the identical stale claim that `converter-dollar-at-migration` carried,
and it is now corrected in both places. Two filings inherited one wrong
sentence, which is an argument for correcting the *filing* when you correct
the code, not just the code.

### Gate 1 — the primary ask, and a documentation bug under it

The tail-recursion example runs (`fact 5 1` → 120, verified). But writing it
up meant stating the policy, and the policy already in the docs was **false**:

> `GUIDE.md:459` — "Depth is static (SB5037 rejects recursion, and functions
> are inlined)."

SB5037 does not reject recursion. It rejects every shape *except* a direct
self tail-call, which compiles to a loop — and `SB5037`'s own message says so
("sash inlines functions, so only a direct self tail-call…"). The diagnostic
was right and the architecture doc was wrong about the thing it was
describing.

Measured boundary:

| shape | result |
|---|---|
| direct self tail-call | compiled to a loop |
| non-tail self-call (`return $(( n * (f m) ))`) | SB5037 |
| mutual recursion | SB5037 |

Added as a `#### Functions, inlining, and recursion (DD#27)` subsection under
Phase 6 in `GUIDE.md`, and as a new `.SH FUNCTIONS AND RECURSION` in
`sash.1` (which had no functions section at all). The corrected sentence at
the capture-depth site now links to it.

### Specs

`rec.1`–`rec.4` in `spec/functions.sh` (plan 98 → 102). These are
**characterization** specs — they pass at HEAD, because they pin behaviour the
docs now claim rather than testing new code. That is the point: a documented
example that nothing compiles is exactly how `GUIDE.md:459` came to assert
the opposite of the truth for however long it did.

`rec.2` is the load-bearing one: 100,000 recursive calls returning 100000.
No inliner survives that, so it demonstrates the loop compilation rather than
asserting it.

Sweep **4508 passing, 9 skipped, 0 failing, 0 flips**.
