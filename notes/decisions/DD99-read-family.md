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
- **`read`'s target kind comes from its declaration**, defaulting to array:
  an undeclared target auto-declares as an array; a `map` target must be
  declared, and consumes fields **pairwise** (`k1 NUL v1 NUL k2 NUL v2`).
- **`$SRC`, if given, is a scalar to split** instead of stdin. This is the
  operation that would otherwise be called `split` — DG's observation that it
  is just a two-argument `read`.

```sh
readline ln                     # one line → scalar
read a                          # stdin fields → array (auto-declared)
map m=(); read m                # stdin fields → map, pairwise
read $ln f                      # split a scalar → array
```

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
| `mapfile -m` | `map m=(); read m` |
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

1. **Odd field count for a map** — a trailing key with no value: reject, or
   bind an empty value? (Recommendation: reject. Silent pairing loss is the
   failure mode this project keeps finding.)
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
