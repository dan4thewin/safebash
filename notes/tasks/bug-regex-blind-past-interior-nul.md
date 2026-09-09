---
title: "bug: regex/gsub silently ignore everything past an interior NUL"
status: ready
priority: P1
kind: bug
depends: []
gates:
  - "`s=\"abc\"; s =~ s/b/$'\\0'/; s =~ s/c/X/` yields `a\\0X`, not `a\\0c`"
  - "`[[ $s =~ cd ]]` on `a\\0bcd` agrees with whatever the replace path does"
  - "no string op on a NUL-bearing scalar returns a wrong answer silently"
---

## What

Every regex-backed string op reads its subject as a **C string** and stops at
the first interior NUL. The bytes past it are neither matched nor replaced —
and are silently retained, so the operation reports success and returns a
wrong answer.

```sh
nul=$'\0'
s="a${nul}bcd"
echo "$(len s)"                  # 5 — the scalar itself is fine
[[ $s =~ cd ]]                   # FALSE. The bytes are there.
u=$(gsub $s "cd" "Z")            # u = "a\0bcd"  — no substitution
w=${s:s/cd/Q/}                   # w = "a\0bcd"  — no substitution
```

Chained edits (DD#96) hit it as soon as one step inserts a NUL:

```sh
s="abc"
s =~ s/b/$nul/, s/c/X/           # "a\0c" — the second edit sees only "a"
```

Affects `[[ =~ ]]`, `gsub()`, `${x:s/…/…/}` and the `var =~ s///` mutation
form. No diagnostic in any of them.

## Why it matters now

Scalars are byte-transparent and **can** hold interior NULs (measured: `len`
is correct, and they survive pipes, redirects and command substitution). DD#99
makes NUL the one field separator, so NUL-bearing scalars go from an oddity to
the normal intermediate value. The idiom the design leans on —
"substitute a separator to NUL, then split" — is exactly the shape that breaks
the moment it needs two substitutions.

Concretely it blocks the `mapfile -m` replacement: turning `k=v` lines into
the flat `k1 v1 k2 v2` array that `map m = @{a:DISP}` consumes needs both the
line separator and the first `=` of each record turned into NUL. Two edits, and
the second one cannot see past the first one's output.

## Where

`sash-rt/src/lib.rs`: `sash_regex_replace` (~1949) and the `sash_regex_match*`
family (~1740-1820) already receive an explicit `str_len` and build a correct
slice — the length is not lost at the boundary. The truncation is one level
down, in the Plan 9 `regcomp9`/`regexec9` API, which takes a `char *` and has
no length parameter.

## Options (needs a decision)

1. **Segment on NUL.** Split the subject at NULs, run the regex on each
   segment, rejoin with the NULs intact. A pattern can then never match
   *across* a NUL — which is coherent with DD#99, where a NUL is precisely a
   record boundary. Contained to the runtime. Non-global (`s///` without `g`)
   needs care: "first match" must mean first across the whole subject, not
   first in every segment.
2. **Make the engine length-aware.** Correct in full generality, and lets a
   pattern span a NUL. Much larger: it means replacing or forking regexec9.
3. **Reject rather than mislead.** A string op on a NUL-bearing subject aborts
   (DD#55) or is a compile error where provable. Smallest and honest, but it
   forecloses the map idiom above, so DD#99 would need another answer for it.

Option 1 looks right — it is the only one that is both contained and leaves
DD#99's idioms expressible — but the "can a pattern match across a NUL"
question is a language-surface call.

## Notes

Pre-existing (the Plan 9 regex has always been C-string based); DD#99 is what
makes it load-bearing. Found while migrating `mapfile -m` sites for DD#99
stage 5.
