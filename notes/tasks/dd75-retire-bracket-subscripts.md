---
title: "DD#75 §4 — retire the [@]/[*] bracket-subscript collection forms; @a is the sole splat"
status: ready
priority: P3
kind: task
depends: []
gates:
  - "${a[@]} / $a[@] (at-bracket splat) → compile error pointing at @a (retires the DD#60 input-alias convenience)"
  - "${a[*]} / $a[*] (star) → compile error pointing at @a (splat) / $(join a) (join); $* removed (see dd75-args-model)"
  - "${#a[@]} / ${#a[*]} → compile error pointing at ${#a}/$(len a); ${!a[@]} / ${!a[*]} → pointing at $(keys m)"
  - "unaffected: ${a[$i]}/${a[1]}/a[$i]= indexed access; @a splat + @{a:-W}/@{a:?W} disposition; * as glob and arithmetic multiply"
  - "sash-convert rewrites the retired bash forms to the canonical spellings (§6)"
  - "0 native/c/janet spec regressions; specs pin each reject + the canonical replacement"
---

# DD#75 §4 — one splat spelling; retire bracket subscripts

Implements DD#75 §4. `@a` becomes the sole array/map splat. Every bracket-`[@]`/
`[*]` collection role has a canonical home already: splat `@a`, join `$(join a)`,
length `${#a}`, keys `$(keys m)`. After this, `@` means only "splat" and `*` has
no array/positional meaning anywhere (only glob + arithmetic multiply).

This retires DD#60's "parser convenience" that accepted `${a[@]}` on input:
one-true-spelling over paste-from-bash (the converter handles bash input, §6).

See DD#75 §4.
