title: DD#28 — No Word-Split Loops
status: implemented

`for x in $choices` is a compile error. Use `for x in "${arr[@]}"`.
Consequence of DD#22 — no word splitting means unquoted expansion
in loop headers cannot split into multiple items.

## Addendum 2026-10-02 — a `$( )` capture is a value, not a stream

The rule was being applied to variables but not to captures. `for x in $(cmd)`
was accepted and lowered to `cmd | while read x`, which split the capture on
newlines — word splitting by another name, and on a different delimiter than
bash uses. It never matched bash: for `$(echo a b c)` bash runs three
iterations and sash ran exactly one, a divergence the spec pinned and called
deliberate.

DG, settling it: *"capturing the whole `$( )` first as though it were assigned
to a variable is to me the least surprising — if I wanted streaming I'd pipe to
`while readline`. I don't think this iterates lines — sash has no word
splitting; read to array and match are the mechanisms for that. Given that,
this looks like a for loop over a single value, which we already have an error
for."*

So a `$( )` for-in source is **one scalar value**, exactly as `v=$( … )` binds
one, and SB5009 rejects it. `$(< file)` goes the same way — it was exempted as
"a stream of lines, like an external command" and already bound the whole file,
newlines and all, in a single iteration.

**The exception set is two intrinsics**, `$(argv …)` and `$(keys m)`, which
genuinely denote collections rather than running anything.

The rule is now stated in the positive (`is_collection_intrinsic`) so it denies
by default. The old spelling (`is_scalar_value_intrinsic`) rejected a named list
and let everything else through, which is why three miscompiles lived under it
unnoticed — see `notes/journal/458-*.md`.

Replacements the diagnostic names: `cmd | while readline x { … }` to stream,
`read -a arr` to split into an array, or `match` to pick a value apart.
