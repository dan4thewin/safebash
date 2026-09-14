title: 300 — `$@` in a function, and a gate that rotted in two directions

`converter-dollar-at-migration` was the next ready task. Its core claim held
up: in-function `$@`/`$*` were entirely unconverted, while the top-level half
had been done long ago. But the filing's treatment of `$#` was wrong twice
over, and catching that was most of the work.

## The gate said `${#args}`. It is `$#`.

The task filed `$#` under "unchanged", parenthesised as "`$#` → `${#args}` per
DD#75 sugar, still valid". Two errors stacked:

1. `${#args}` is **SB4038**. DD#75 §4 retired the `${#…}` length sugar *after*
   this task was written.
2. The obvious repair — `$(len args)`, which is what I implemented first — is
   also wrong. DD#46 says `$#` is genuinely function-local **and sash-native**:
   "they are scalar, so their `$` sigil is consistent." `fn f { echo $# }`
   compiles. So does `$(( $# + 1 ))`. I checked, because by then I had stopped
   trusting anything written down.

Bare `$#` needs no conversion. The only `$#` that moves is the braced `${#}`,
which is SB4034 — inside `${…}` the `#` reads as the length operator sash does
not have.

What caught the mistake was not review. It was three specs failing:
`conv.f1`, `conv.f3`, `conv.x8` all asserted that in-function positionals pass
through untouched. I had been about to update all three as stale pins of a
closed gap. `conv.f1` genuinely was one — the DD#46 amendment of 2026-08-26
rejects `$@`/`$*` everywhere, so a function body does need `$(join args)`, and
that spec had been pinning the unconverted state. But `conv.f3` and `conv.x8`
were **correct**, and they were correct about the exact thing I had just got
wrong. The habit of reading the DD before overriding a spec is the only reason
this did not ship.

This is the third time in four days that a filing has been materially wrong
about the thing it was filed to fix. The pattern is consistent enough to state
plainly: **a task file records what was true when someone noticed a problem,
and nothing ages it.** Re-measure before implementing, every time.

## What the conversion actually needed

Context decides the target, and there are three of them:

| bash (in a function) | sash | why |
|---|---|---|
| `for x in "$@"` | `for x in @args` | splat, quotes must go |
| `g "$@"` | `g @args` | argument list |
| `echo "$@"` | `echo "$(join args)"` | `echo` joins — a splat is SB5030 |
| `j="$@"` | `j="$(join args)"` | a value, not an argument list |
| `"pre $@ post"` | *MANUAL* | no sash form exists |
| `"$*"` / `${*}` | `$(join args)` | always joined |

Two of those rows are traps.

**The assignment row.** tree-sitter-bash puts a `variable_assignment` inside a
`command` node, so walking up for "am I an argument?" finds one and emits a
splat where a joined string is meant. My first `enclosing_command_name` did
that — and then collided by name with one already at `convert.rs:1920` that has
exactly the permissive walk. Renamed mine `argument_command_name` and left a
comment saying why the strict version exists, since the two look
interchangeable and are not.

**The embedded row.** `"pre $@ post"` word-splits into three words in bash.
sash has no such form, and the tempting output — `"pre @args post"` — is a
plain literal, silently. `splat_replace_node` only unwraps a *sole*-content
string, so it hands the node back unchanged here, which is the signal to emit
a MANUAL instead of a conversion. Except for `echo`, where joining is what
`echo` does anyway and `"pre $(join args) post"` is exactly faithful.

Unquoted `$@` converts but gets a CHECK: bash re-splits each argument on IFS,
`@args` does not. `f "a b"` reaching `g $@` passes two arguments in bash and
one here. Worth saying, though sash catches the shape that matters — it is
SB5042, wrong arity, not silent divergence.

## A diagnostic that recommends a rejected form — second instance

SB5030 told the user to write `echo $(join @a)`. That is **SB5062**. `join`
takes the array by name: `$(join a)`.

DD#100 line 119 already knew, and deferred the cleanup into
`dd75-len-collection-only-strlen` — which is marked **done** with that part
unmet. So the bug survived being found, being written down, and being assigned.

This is the same defect as journal 299's SB4035 (which recommended `${a[i]}`
while the parser rejected it). Two instances in two days is a class, not a
coincidence, and I have filed `audit-diagnostic-suggested-spellings` to walk
every message containing a suggested spelling and compile it. `gs.36` now pins
SB5030's advice by asserting the message text, and `gs.37` compiles and runs
the form it recommends — pinning advice without checking it runs is how this
class persists.

## Verification

Fail-first predicted 12 failures — 11 in `convert.sh`, 1 in `glob_splat.sh` —
and named which. Observed exactly that set. `57.130` and `gs.37` were predicted
to pass *without* the change (they assert forms that already worked) and did,
which is the half of a fail-first run that is easy to skip and worth keeping.

Sweep: **4481 passing, 9 skipped, 0 failing, 0 flips** against `a231fb9`.
Predicted 4481 (4465 + 12 native-only + 2×2 dual-mode) before running.
