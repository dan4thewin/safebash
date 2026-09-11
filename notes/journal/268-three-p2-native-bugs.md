# 268 — Three P2 native bugs

DG: "fix P2 native bugs now". The census had turned up exactly three P2s in
the native compiler — `sb5021-stale-advice-mapfile`,
`bug-match-outarray-no-autodeclare`, and
`read-family-redirect-open-not-guarded`. All three are fixed. Sweep
**5001 / 38 / 512**, zero non-janet failures, zero flips against `ea0e7a8`
and `0ca316a`, and all 48 new test rows passing or deliberately skipped.

Each bug had something wrong with its own filing, in a different direction.

## SB5021: the message was the bug, and so was the map

```
$ m[foo]=$(/usr/bin/printf zz)
error [SB5021] tainted value cannot be placed in array 'm' — external data
enters arrays via read/mapfile; or attest a scalar first and assign the
clean value
```

`mapfile` is retired, so following the advice gets you SB5060. And `m` is a
map, which the message calls an array, and for which the suggested route does
not exist — under DD#99 external data reaches a map only through a
vouched-for array.

The interesting part was that the validator had no idea `m` was a map.
`SlotInfo`/`SlotCx` carry no collection kind, and `Assignment` has no kind
field, so the distinction had to be threaded in from the declarations. I was
about to write that classification when I found `type_check::
collect_collection_kinds` already doing it — `-A` vs `-a`, exactly. Made it
`pub(crate)` and reused it. `SlotCx` gained a `map_names` set and an
`is_map()`, and the four write shapes (array subscript, map subscript, `array`
literal, `@{…}` literal) now all route through one
`tainted_into_collection_msg(name, is_map)`.

Before writing the replacement text I ran every route it recommends:
scalar-attest into an array element, scalar-attest into a map element, and
`read` → `attest` → `map m = @{p:~}`. All three work. That is the whole point
of the fix — the old message's advice did not.

## `match`: three dispatch sites, and there were five

The filing named `collect_read_targets`, the transform, and the type checker.
`string_literals` and `lines_fts` found two more in the janet emitter, plus
`command_writes_args`. `matchall` had been added by DD#63 *after* the filing
and had the same gap in two of them.

All five now read `"match" | "matchall" if c.args.len() == 3`. The arity guard
matters: the 2-arg boolean form must keep binding nothing.

## The read redirect: the filing guessed the root, and was right

> Likely the same root as the note in `is_bulk_read_cmd`: the bulk reads lower
> to SSA instructions that carry no redirect list.

Exactly that. `read a < f` was bracketed by `RedirectPush`/`RedirectPop` built
from the *unguarded* `emit_redirects`; `emit_readline` had its own copy of the
same omission.

Two places to fix it, and I went looking for which before choosing: guard
inside `RedirectPush`, or give `ReadArray` its own redirects the way
`ReadLine` already has. What settled it was finding that `RedirectPush` is
reachable **only** via bulk reads — SB4021 rejects redirects on compound
commands at parse and SB4046 rejects them on declarations and assignments, so
the `is_compound_stmt` branch in `lower_redirected_stmt` is effectively dead
for redirects. And push/pop bracket an arbitrary span of blocks with no body
to skip, so the "skip the body, flow the status to the condition" half of
DD#55 could not be written there at all. The symmetric fix was the only one
that could express the behaviour. Internal-representation fork — decided and
reported.

Then `in_condition` never reached the reads: `codegen/entry.rs::
emit_block_insts` scanned only `Command` and `Pipeline` when deciding whether
the last instruction sits in a condition. Extended to the three read insts, and
`if read a < nosuch` catches.

One thing I did not fix. A failed open in a `while` **condition** aborts in
native and is caught in c. It reproduces at `c1e9807` with a plain `echo` and
no reads at all, so it predates this work, and whether a `while` condition
should catch is a DD#55 behaviour question — filed as
`dd55-while-condition-redirect-divergence` rather than decided.

## Fail-first, honestly

Stashed `sash/src`, rebuilt, ran the three spec files. 55.60-55.62,
60.56-60.59 and 63.mi1-63.mi4 all failed unpatched. 55.63, 55.64, 55.65,
55.66 and 63.mi5 **passed** pre-fix — they are regression pins, not
discriminators, and the spec says so in a comment rather than pretending
otherwise. 55.63/55.64 cannot discriminate because the spec harness runs the
binary with `</dev/null`, so a test that needs stdin is not available to them.

Two of my own expectations were wrong and the sweep caught both: `- echo hit`
suppresses the **status check**, not the output, so 63.mi2 and 63.mi4 were
missing a `hit` line I had assumed was swallowed.

## Two janet bugs surfaced, neither caused

`janet:63.mi1-mi3` fail because `emit_match_into_condition` calls
`predeclare_one` mid-expression, so the auto-declaring `var` lands inside the
if-condition: `(if (var g @[]) …)`, which janet rejects. Reproduces with
`matchall` on untouched code.

`janet:55.65/55.66` fail because janet drops the input redirect on the read
family entirely and reads the process's stdin — which the harness supplies as
`/dev/null`. The janet backend emits reads from the IR as *commands*, never
passing through the SSA instructions that now carry redirects, and nothing in
that arm looks at `c.redirects`. Verified pre-existing: the `emit_janet.rs`
diff in this change touches only the `match` paths.

Both filed (`janet-match-outarray-var-in-condition`,
`janet-read-family-ignores-input-redirect`) and gated `native,c`. Probably both
go away with `emit-janet-from-ssa`.

## Rust suites

140 failures across the five suites that complete — the documented baseline,
compared by name and not only by count. `codegen_tests` still hangs
(`rust-tests-hang-on-retired-surfaces`, P1, two forks open for DG).
EOF
