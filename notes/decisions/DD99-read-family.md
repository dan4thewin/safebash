---
title: DD#99 — the read family: `read` / `readline`, NUL-only, no flags
status: accepted
---

Settled with DG over 2026-09-08, from his observation that `read` and `mapfile`
*"are hovering around the same set of operations and the distinctions are
historical/arbitrary"*, and his proposal: **"split only on NUL, terminate read
on newline"**, with the user converting their separator upstream.

Accepted 2026-09-09 (DG: *"let's implement dd 99"*). Implemented in stages —
see `notes/tasks/dd99-implementation.md`.

## The rule

Two commands. No flags.

```sh
read [$SRC] TARGET      # consume NUL-separated fields
readline TARGET         # consume one line (terminated by newline)
```

- **NUL is the only separator.** Nothing else splits, ever.
- **`readline` stops at a newline; `read` does not.** That is the only
  difference between them.
- **`readline` binds a scalar.** One record is one value.
- **`read` always produces an array.** No declaration-dependent behaviour, no
  second target kind. An undeclared target auto-declares (DG, 2026-07-08).
- **`$SRC`, if given, is a scalar to split** instead of stdin. This is the
  operation that would otherwise be called `split` — DG's observation that it
  is just a two-argument `read`.

```sh
readline ln                     # one line → scalar
read a                          # stdin fields → array (auto-declared)
read $ln f                      # split a scalar → array
```

## Maps are built by assignment, not by `read`

An earlier draft had `read` consume fields pairwise when its target was a
declared map. DG rejected it, and rightly: that is **implicit polymorphism** —
you cannot tell what `read m` does without finding `m`'s declaration elsewhere
in the file. It is the same defect DD#97 had just removed from bare `f=(*)`.

Maps are made by converting an array:

```sh
read a                              # always an array
if (( (len a) % 2 == 0 )) { … }     # optional checkpoint — works today
map m = @{a:-EXTRA}                 # pairwise: k1 v1 k2 v2 …
```

The array is consumed **pairwise**: element 0 is a key, element 1 its value,
and so on. An **odd** count leaves a trailing element with no partner, and the
splat's disposition says what to do about it.

DG: *"There'd be an opportunity for the user to check if it were even between
the read and the assign."* Under the rejected one-step form there is no array
to inspect — so the two-step is what makes checking possible at all.

Array→map is also independently useful: build an array programmatically, then
convert.

### The disposition is mandatory

As with DD#69's external-command splat, a bare `map m = @a` is a **compile
error**. The hazard — a silently mis-paired map — is exactly the shape DD#69
addressed for a silently-empty argument list, so the same rule applies: the
splat site states its behaviour.

| form | on an odd count |
|---|---|
| `map m = @{a:-KEY}` | the trailing element becomes the **value** of `KEY` |
| `map m = @{a:?MSG}` | abort at runtime with `MSG` |
| `map m = @{a:~}` | discard the trailing element |
| `map m = @{a:!~}` | discard it **and** disable the taint guard (DD#91) |
| `map m = @a` | **rejected** — state a disposition |

**`:-` supplies a key, not a value** (DG): *"If I supply the key, I know what
key has the extra value. If I supply a value, I'd have to iterate to find the
extra."* So `array a=(k1 v1 k2)` with `@{a:-EXTRA}` yields
`{k1: v1, EXTRA: k2}`, and the odd element is retrievable at `${m[EXTRA]}` —
a landmark rather than a needle in the map.

Each disposition keeps the meaning it has everywhere else — `:-` supplies what
is missing, `:?` aborts with a message, `:~` elides, `:!~` elides with the taint
guard off. Only the definition of *missing* is contextual: an empty array in
argument position, an unpaired trailing element here.

**The disposition governs odd count only.** An empty array is a valid empty map
(zero is even; nothing is missing), so DD#69's empty-array reading does not
apply in this position. Without this, the two rules collide: `@{a:-X}` on an
empty array would yield one element under DD#69, making the count odd, which
would then re-trigger the same disposition.

## Sigils: a value carries one, a binding does not

DG: *"when I see `@` and `$`, I think those are operating on the **values** of
the array or scalar."* The rule that falls out:

> **A sigil yields a value. A bare name is a reference.**

| position | form |
|---|---|
| target (a binding) | `read a`, `readline ln`, `read $s a` |
| collection operand | `len a`, `join a`, `keys m` |
| value | `$x`, `@a` |

This matches what targets already do (`read -a @a` does not bind `a`). It also
gives `bare-collection-name-operand-asymmetry` a direction: bare-as-reference
is canonical, and `len @a` — accepted today — is the redundant spelling.

## Why NUL only

Because it is the one byte that cannot occur in the data, which is the whole
`-print0` tradition. But the deeper reason is DG's:

> *"Philosophically, requiring thought about delimiters (changing whitespace to
> NUL) might be a feature."*

That is not a new principle. **DD#22 is titled "No Word Splitting, No IFS"** and
says *"No implicit splitting. IFS doesn't exist."* The read family is where that
leaked: `read a b c` splits on whitespace implicitly — `x=[a] y=[b c d]` — which
is IFS surviving in the one place DD#22 did not police. DD#22's own taxonomy
gave field splitting an **explicit** delimiter (`read -d: -r a b c`).

`-f` appears **nowhere** in DD#22. It is post-DD drift, and `-d` was reassigned
from field to record delimiter to make room for it. The `-d`/`-f` collision — a
`read -d:` that silently truncates when the user meant `read -f:` — is the scar
from that drift. This DD does not invent a rule; it finishes DD#22.

## What is retired

| retired | replacement |
|---|---|
| `read -a arr` | `read arr` (kind from the declaration) |
| `read -f<c>` | convert upstream: `tr '<c>' '\0'`, or in-process `s =~ s/<c>/$sep/g` |
| `read -d<c>` | `read` (no terminator) or `readline` (newline) |
| `read -r` | always raw; the flag was already a no-op |
| `read a b c` (multi-target) | `read arr`, or `readline` + `read $ln f` |
| `mapfile` / `readarray` | `tr '\n' '\0' < f \| read arr` — NUL is the only separator, so a *line* split states itself |
| `mapfile -m` | `tr … \| read a; map m = @{a:DISP}` |
| `mapfile -f<c>` | pairwise consumption needs no second separator |
| `mapfile -t` | always trims; the flag was already a no-op |

Also: `echo -n` is spelled **`write`** (DG). `echo` terminates, `write` does
not, neither takes a flag.

## Everything needed already exists

Verified 2026-09-08 against `target/release/sash`:

| primitive | status |
|---|---|
| `$'\0'` | ✅ ratified by **DD#64** ("the one place the shell reads backslash escapes") |
| a scalar holding NUL | ✅ `len=5` for `a\0b\0c`; better than bash, which drops NULs |
| producing NUL in-process | ✅ `$(gsub $s ":" $'\0')` |
| NUL through pipes/files/capture | ✅ byte-transparent |
| NUL-split into an array | ✅ this is `read -a -d ""` today |
| one line → scalar | ✅ this is `read line` today |
| auto-declared targets | ✅ DG 2026-07-08, `autodeclare_bulk_target` |

So this DD is **naming and subtraction, not construction**.

## Why `readline` binds a scalar (and not an array)

The alternative — `readline` also NUL-splitting, into an array — is more
symmetric: two commands differing in exactly one axis. DG framed the choice as
two empirical questions. Measured over 536 real shell scripts (76k lines,
~64 genuine `read` sites):

| question | answer |
|---|---|
| Is **splitting** the common case? | **No — ~17%.** Single-target is ~83%. |
| Is **line-at-a-time** the common case? | **Yes — 79 `while read` vs 8 `mapfile`,** ~10:1 |
| Which separator do splits use? | Almost all default whitespace; 2 use `-d` |

`readline` is the dominant construct by an order of magnitude, so its
ergonomics carry the decision, and ~83% of its uses do not split. An
array-binding `readline` would tax every one of those with `${ln[0]}` and
attest-all, to save a line on the 17% — which DG assessed as "more ergonomic
than B, but only a little".

Two incidental findings support this. In the wild:
`read _x _x _x _x disk_size _x sector_num _x` — the discard pattern, hand-rolled
(DD#98's `_` reinvented); and `IFS=: read -t 0.1 -d $'\a' x fg` — field
separator, record delimiter and `$'…'` in one call.

**Caveat on the evidence:** this is bash in the wild, and 83% single-target
partly reflects that whitespace splitting is *free* in bash. Under NUL-only it
would not be, which cuts slightly toward the array form. It does not move the
10:1 ratio, which is the load-bearing number.

## Costs, stated plainly

- **~78% of real splits are whitespace splits** that today need no thought.
  Each becomes an explicit conversion. That is the point of the design, but it
  is a real cost paid at many sites, not a free win.
- **`k=v` map building is the awkward case**: both `=` and the newline must
  become NUL. And `s =~ s/=/$sep/g` is currently broken
  (`sed-sugar-ansi-c-quoted-replacement`) — the function form
  `$(gsub $s "=" $sep)` works. Worth a worked example before ratifying.

## Resolved (DG, 2026-09-09)

### The inverse conversion is rejected, not built

`array a = @m` (map → flattened pairs) is a **compile error**. It parses today
and silently yields an empty array, so something must change; rejecting is the
cheap, reversible move.

It is *well-defined* — sash maps preserve **insertion order** (verified: a map
built `zebra apple mango kiwi` iterates in that order, stably across runs), so a
flattened array would round-trip deterministically. And it is *not needed*: a
map is already emittable with the keys loop,

```sh
for k in $(keys m) { /usr/bin/printf "%s=%s " $k ${m[$k]} }
```

so flattening is a convenience, not a capability. Because the ordering
semantics are settled, it can be added later without reopening any question.
Building it now would be symmetry for its own sake.

### Interior NUL crossing argv: a runtime check, not static analysis

A NUL-bearing value passed to an external command **silently truncates**
(`printf "[%s]" $x` on `a\0b\0c` → `[a]`). Pipes, redirects and captures are
all byte-transparent; argv alone is not, because a C string is unavoidable
there. Under a NUL-centric design this stops being exotic.

DG: *"I don't want silent truncation of arguments, but it seems too unlikely to
merit the same defensive posture we usually apply."* Agreed — and it needs no
static analysis, because the test is exact and local:

> Every sash string is `(ptr, len)` and, per **DD#87**, NUL-terminated at
> `[len]`. An interior NUL is precisely `memchr(ptr, 0, len) != NULL`.

**Rule:** at the argv build site, an argument containing an interior NUL
**aborts** (DD#55) with a diagnostic naming it. One `memchr` per argv entry per
exec — negligible beside the `fork`/`execve` it precedes. No false positives,
no false negatives, no analysis. Silent truncation becomes a loud abort for a
handful of lines in the runtime's exec path.

### Migration: convert mechanically

sash is private and unreleased, so breaking changes are acceptable (DG). The
old spellings are removed outright — no deprecation period, no dual forms.
`read -f:` has 4 in-tree uses and `read -a` 26; the multi-target `read a b c`
form is widespread in `spec/` and converts mechanically.

## Related

- Retires the open decisions in `switch-free-read-family`.
- Depends on nothing; blocked by nothing. The bugs found while designing it
  (`bug-while-read-a-zero-iterations`, `bug-sed-op-on-array-empties-it`,
  `bug-match-outarray-no-autodeclare`, `sed-sugar-ansi-c-quoted-replacement`)
  are all independent and should be fixed on their own terms.

## Stage 6 landed (2026-09-10) — the rename, and the old surface retired

`newread` was only ever a staging alias so nothing changed meaning while code
still depended on it. It is gone: **`read` IS the array form**.

```sh
read ARR            # stdin, NUL-separated fields -> array
read $s ARR         # split a value already in hand
readline v          # one line -> scalar
```

Retired, each with a diagnostic rather than a silent reinterpretation:

| gone | code |
|---|---|
| every read flag (`-a -d -f -r -z …`) | SB5057 |
| multi-target `read a b c` | SB5057 |
| `mapfile` / `readarray` | **SB5060** (new) |

SB5060 names the replacement instead of letting the retired builtins fall
through to SB5011 "command not found", which would send the author looking for
a missing binary. `readarray` had to be *added* to the builtin registry to get
there — it was never in it, so SB5011 spoke first.

`-r` is worth calling out: it used to be an accepted no-op, because sash reads
are always raw and bash scripts write `read -r` everywhere. DD#99 takes no
flags at all, so that courtesy is gone.

### Three defects the rename exposed, all mine

- Dropping `read` from `is_bulk_read_cmd` made `read ARR < f` **ignore its
  redirect** and read stdin instead — empty array, no diagnostic, and a
  missing file did not even abort.
- The lastpipe classifier still treated `read` as the *scalar* consumer, so a
  piped `read ARR` produced the wrong element count.
- `command_writes_args` matched `"read"` on its old flag-based arm before the
  new one, so `read args` stopped being SB5052 and reached the linker.

Each was caught by a spec, which is the argument for having migrated the
vehicles in stage 5 first.

### Not done

`sash-convert` still passes bash `read` through verbatim, so its output no
longer compiles. It has no command-rewriting path today (it only inserts
annotations), so this is an addition rather than an edit — filed as
`sash-convert-read-dd99` (P1). `docs/sash.1` still documents the retired flag
surface.

## Stage 7 landed (2026-09-10) — the argv interior-NUL guard

argv crosses `execve(2)` as C strings, so a NUL-bearing value was truncated
there: silently, with a zero exit, and the callee received a **different,
shorter argument** than the program computed.

```sh
s="abc$'\0'HIDDEN"
echo "$(len s)"          # 10
/usr/bin/printf "[%s]" $s # [abc]   ← 7 bytes gone, rc 0
```

DD#99 is what makes this urgent rather than exotic: scalars are
byte-transparent and NUL is now *the* field separator, so "read a
NUL-separated stream, forget to split, pass the whole thing" is an ordinary
slip. Truncation is the one outcome the language must not have, so
`sash_argv_guard(ptr, len)` aborts (DD#55).

The check has to sit where argv is **built**, not in `sash_exec` — by there the
length is already gone. Three sites, all of them:

- the fixed (stack) argv path;
- the runtime path's scalars and `:~` elides;
- **splat elements**, via the length `sash_table_value` already writes out.

That last one mattered: guarding only scalars would have been *worse than no
guard*, because it would imply argv NULs abort while `@a` still truncated
quietly.

A NUL remains a perfectly good value in-process — spec an.5 pins that, since
it is the basis of the whole design.

Specs `commands.sh` an.1-an.5 (native + c; janet has no `$'…'`/split path).

## Stage 8 landed (2026-09-10) — `array a = @m` is a compile error (SB5061)

It was **accepted**, and silently produced an **empty** array — the same
"accepts input and gives an undefined answer" shape as the ICE batch and the
`f=(*)` collection bug.

A map is built FROM an array (`map m = @{a:DISP}`). The reverse was never
specified, and picking a flatten order on the author's behalf is exactly the
silent mis-pairing that DD#69 and DD#99 make you state a disposition to avoid.
Adding a disposition does not rescue it (`array a = @{m:~}` is equally
rejected) — the direction is the problem, not the emptiness policy.

To walk a map, iterate it: `for k in $(keys m)`.

**Reasoning replaced (2026-09-11).** DG later ruled that a splat yields
**values, not indices** (DD#75 clarification), so `@m` is `v1 v2` and there is
no pair for a flatten order to get wrong — the "silent mis-pairing" argument
above no longer applies. The verdict stands on a better reason: the **round
trip is broken**. `array a = @m` sits opposite `map m = @{a:DISP}` and reads as
its inverse, but re-mapping the values cannot rebuild the map —
`(k1 v1 k2 v2)` → 2 entries → values `(v1 v2)` → 1 entry keyed `v1`. Rejected
because it looks like something the language does not do, which is the same
rule that motivates SB5071 (DD#86).

Specs `var_modifiers.sh` af.1-af.4 (all three modes; af.4 is the
supported-direction control, gated for `janet-map-from-array`).

## Status

Stages 1-8 are complete. Remaining, both filed rather than folded in:

- `sash-convert-read-dd99` (P1) — the converter still passes bash `read`
  through verbatim, so its output no longer compiles. It has no
  command-rewriting path today, only annotation insertion.
- `docs/sash.1` still documents the retired read flag surface; that is mostly
  deletion rather than correction.

## Clarification (2026-09-11) — `while read ARR` is SB5069

DG: *"`while read ARR` seems nonsensical so reject. counter argument?"*

The counter-argument worth having is bash's `while read -a arr`, which splits
each **line** into fields. If sash had an equivalent, `while read ARR` would be
its natural spelling and rejecting it would remove a real idiom.

It does not. `read $s ARR` splits on **NUL** — that is the rule at the top of
this DD, deliberately. On `"a b c"` it yields one element, not three. There is
no per-line field split in the language, so there is nothing for
`while read ARR` to be the spelling *of*.

What it does mean is: slurp the whole stream, then ask the loop to go round
again on a stream that is now empty. Either it never terminates, or it is a
one-shot dressed as a loop. `readline v` is the iteration form and the
diagnostic names it.

Scope of the rejection:

- fires on the **condition** of `while` *and* `until` — the defect is looping
  on a slurp, not the sense of the test
- walks the condition, so `tr … | read ARR` is caught as well as the bare form
- `if read ARR` is still **accepted**: asking whether a read produced anything
  is meaningful. Only iterating on it is the category error.

`read ARR` in a loop **body** remains legal. It was segfaulting — the empty
collection was allocated during a discarded first lowering pass — and that is
fixed in the same commit (journal 265).

Specs `spec/commands.sh` 14.37–14.46.
