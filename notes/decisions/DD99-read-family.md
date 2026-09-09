---
title: DD#99 — the read family: `read` / `readline`, NUL-only, no flags
status: proposed
---

Settled with DG over 2026-09-08, from his observation that `read` and `mapfile`
*"are hovering around the same set of operations and the distinctions are
historical/arbitrary"*, and his proposal: **"split only on NUL, terminate read
on newline"**, with the user converting their separator upstream.

Status is `proposed`, not `accepted`: three questions at the end are open.

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
| `map m = @{a:!}` | discard it **and** disable the taint guard (DD#91) |
| `map m = @a` | **rejected** — state a disposition |

**`:-` supplies a key, not a value** (DG): *"If I supply the key, I know what
key has the extra value. If I supply a value, I'd have to iterate to find the
extra."* So `array a=(k1 v1 k2)` with `@{a:-EXTRA}` yields
`{k1: v1, EXTRA: k2}`, and the odd element is retrievable at `${m[EXTRA]}` —
a landmark rather than a needle in the map.

Each disposition keeps the meaning it has everywhere else — `:-` supplies what
is missing, `:?` aborts with a message, `:~` elides, `:!` elides with the taint
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
| `read -f<c>` | convert upstream: `$(gsub $s "<c>" $sep)` or `tr` |
| `read -d<c>` | `read` (no terminator) or `readline` (newline) |
| `read -r` | always raw; the flag was already a no-op |
| `read a b c` (multi-target) | `read arr`, or `readline` + `read $ln f` |
| `mapfile` / `readarray` | `read arr` |
| `mapfile -m` | `read a; map m = @{a:DISP}` |
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

## Open questions

1. **The inverse conversion.** `array a = @m` (map → flattened pairs) also
   parses today and silently yields an empty array. If array→map exists, does
   its inverse, and with what disposition family? Deciding both together seems
   right.
2. **Interior NUL crossing argv.** A NUL-bearing value passed to an external
   command **silently truncates** (`printf "[%s]" $x` on `a\0b\0c` → `[a]`).
   Pipes, redirects and captures are all byte-transparent; argv alone is not,
   because a C string is unavoidable there. Under a NUL-centric design this
   stops being exotic. Guard statically where the value is known, at runtime
   otherwise, or document it?
3. **Migration.** `read -f:` has 4 uses in-tree and `read -a` 26, but the
   multi-target `read a b c` form is everywhere in `spec/`. Convert
   mechanically, or keep the old spellings deprecated for a period?

## Related

- Retires the open decisions in `switch-free-read-family`.
- Depends on nothing; blocked by nothing. The bugs found while designing it
  (`bug-while-read-a-zero-iterations`, `bug-sed-op-on-array-empties-it`,
  `bug-match-outarray-no-autodeclare`, `sed-sugar-ansi-c-quoted-replacement`)
  are all independent and should be fixed on their own terms.
