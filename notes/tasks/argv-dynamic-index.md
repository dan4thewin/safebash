---
title: "argv with a dynamic slice start (`argv $n a`)"
status: wontfix
priority: P3
kind: feature
depends: []
gates: []
---

# `argv $n a` — a dynamic slice start — **wontfix**

`argv N a` (DD#60) takes a **literal** N. `SsaInst::ArgvFill { array, start:
i64 }` carries a compile-time constant, so there is nowhere to put a runtime
value.

Until 2026-09-22 a sigilled index was not rejected — it was *mislowered*.
`try_handle_argv` returned `None` for any shape it could not lower, and `None`
in `try_builtin` means "not a builtin", so `argv $n a` became an ordinary
command named `argv`, compiled clean, and the binary died with `sash: bug:
bare command passed to sash_exec`. SB5085 now rejects it at compile time.

## DG's ruling (2026-09-22)

**No.** A compile-time-constant fill start is the rule, not a limitation
awaiting a fix. The diagnostic offers the alternatives instead, and the advice
is deliberately **generic** — one message that reads correctly both for the
user who merely over-quoted a literal (`argv "1" a`) and for the user who
wanted a runtime start (`argv $n a`). Splitting it would have been the kind of
input-specific advice that went wrong in SB4064 (journal 389).

Both named alternatives were measured against the live binary before being
written into the message — a slice with a runtime start is expressible today:

```sh
# index the whole array at a runtime position
array a
argv a
attest v in a [[ -n $v ]]
echo ${a[$n]}

# or loop and skip
for x in $(argv)
do
  attest x [[ -n $x ]]
  ...
done
```

`$(arg $n)` already accepts a dynamic index for the single-element case
(spec `av.t8i` — "a COMPUTED index is a runtime concern, not a typo"), so the
scalar half of this was never missing.

Shields: `av.f9` (sigilled index rejected), `av.f4`/`av.f5` (quoted),
`av.f3` (bracketed), `av.f10` (control: a literal start still lowers).
