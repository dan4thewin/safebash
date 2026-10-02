# 460 — the arity was the whole discriminator

DD#75 §3 had promised, for a long time, that a map's keys could be *"iterated
or snapshotted into an array"*. Only the first half existed. `$(keys m)` was
legal in exactly one position — a `for`-in header — and nowhere else, so a
script that wanted the keys twice, or wanted them in a stable order across a
write to `m`, or wanted to pass them somewhere, had to walk the map again and
hope nothing had changed in between.

DG picked the out-param: **`keys m k`**, mirroring DD#60's `argv [N] a`.

## What it cost, and why that number is interesting

Fourteen files. That sounds like a lot for one builtin form, and it was — but
almost none of it was the *feature*. The runtime function is nine lines. The
SSA instruction is two fields. The transform arm is one `match` entry. The two
backends are a call each.

The rest was **registration**. `keys` had to be added, by name, to:

- `collect_read_targets` — so `k` counts as written
- `insert_auto_collection_target` — so a bare `k` auto-declares as an array
- `flag_fill_target` — so `keys m "k"` is SB5063
- `command_writes_args` — so `keys m args` is SB5052
- `type_check`'s `check_command` dispatch — so `k` gets a collection type
- and removed from `VALUE_ONLY_INTRINSICS`, which had been carrying the whole
  "`keys` is not a statement" rule on its own

Six by-name registries, none of which can tell that a seventh fill command
exists. Nothing in the type system forces a new fill to join them; the only
reason all six got done is that I went looking for `ArgvFill` in the index and
read every hit. That is the same shape as journal 456's private-walker-copy
and 441's second implementation: **the invariant lives between functions, so
no function owns it.**

I did not catch all six by reading. `command_writes_args` I found by *probing*
— deliberately trying reserved names after the feature already worked. `keys m
args` compiled and ran, writing to the immutable function-args array, while
`argv args` and `read args` have been SB5052 for months. A feature that works
is not a feature that is finished.

## The arity was the whole discriminator

`argv` carries a walker family — `check_argv_form_*`, `cmdsub_is_argv_splat`,
`is_bare_index_literal`, `argv_target_name` — for one reason: `argv a` and
`argv 1` are both **one argument**, and only the shape of that argument says
which form you wrote. Something has to arbitrate.

`keys` has no such collision. One argument is the splat, two is the fill. I
spent a while planning the `keys` equivalent of that walker family before
noticing there was nothing for it to decide. What shipped is a single function,
`keys_form_diag`, hung off `check_command_head`'s existing `CmdPos` — which
already knows every position a command can appear in, and was already being
called for `keys`.

The lesson is not "keys was easy". It is that **I reached for the shape of the
nearest neighbour before checking whether the thing that made the neighbour
complicated applies here.**

## The snapshot is clean — checked, not cited

`argv a` is tainted (DD#61). The obvious move is to mirror that, and it would
have been wrong: `keys m k` is **clean**.

The reasoning is DD#93's — argv's bytes come from outside the program, a map's
keys were put there by it — and a session earlier I would have simply cited
that. Journal 459 is the record of what citing it was worth: DG asked for the
counter-example, and there were three, all of them bugs in the rules that were
supposed to be making the claim true.

So this time I enumerated the routes to a map key and ran each one:
`m[$(getenv HOME)]="v"` → SB5020; the same value attested first → accepted,
and clean, because that is what attest is *for*; `@{a:!~}` → clean by the
DD#91 vouch, which is the author taking responsibility explicitly; `@{a:~}`
and `@{a:-X}` → SB5020; `read m` and `argv m` → SB4030, no bulk fill reaches a
map at all. Every route is a literal, an attested value, or an explicit vouch.

That is also why `keys` is deliberately **absent** from `bulk_fill_targets`,
which looks like a seventh registry it should have joined. That list means
"this command writes bytes from outside the program" and feeds SB5067. `keys`
does not.

## Two doc comments the change falsified

Both found by reading, neither by a test — a test cannot fail on a comment.

`cmdsub_arity_diag` said *"`match` is the only name that is both a value
intrinsic and a legal statement."* True when written; `keys` now joins it. The
visible consequence was that `$(keys m k)` drew the generic *"a value
intrinsic must be called with its exact arity"*, which is useless advice when
the problem is that the form produces no output at all. `keys` now gets the
same treatment `match` already had.

`sole_cmdsub_command` explained why `keys` escapes a defect it describes. Still
true — but the rule that catches it moved out of `VALUE_ONLY_INTRINSICS` and
into `keys_form_diag` in this commit, so the comment was pointing at an empty
room.

## Numbers

16 specs (`types.sh` 75.132-75.147), `plan` 307 → 323. The sweep predicted
7377 and ran 7377. `mk test` green.

One refactor fell out of the complexity gate: `flag_fill_target` reached cog 6
against a 5.4 ceiling once `keys` was added. It split into `fill_target_spec`
(the by-name table, a flat dispatcher at cog 1) and `is_valid_fill_target` (the
bare-name-or-`a[i]` predicate), leaving the loop at cog 3. The gate did its
job: the function had been accumulating by-name arms for four builtins and was
the right thing to break up.
