---
title: "DD#91: does `!` vouch for a taint SOURCE materialised in the `:-` default?"
status: done
priority: P2
kind: question
depends: []
gates:
  - "DG decides whether `${x:!-$(getenv HOME)}` is legal"       # DONE — reading 2
  - "whichever way: the answer is a spanned diagnostic or a clean compile, never the bare SSA backstop"  # DONE — SB5020
---

# The question

DD#91's `!` is a per-use taint bypass: `${t:!}` reads a tainted variable and
yields a clean value, no predicate, no attest. That much is settled and
specced (`61.9`).

What is *not* settled is what `!` vouches for when the expansion also
materialises a **fresh source** in its `:-` default:

```sh
str x=""
str z=${x:!-$(getenv HOME)}
```

Two readings, both defensible:

1. **`!` vouches for the whole expansion's value.** The author has said "trust
   this result", and the result here is the env var. It should compile clean,
   and today's rejection is a bug.
2. **`!` vouches only for the read of the named variable.** `x` is what the
   bypass is about; `$(getenv HOME)` is a *new* entry of external data into the
   program, which no bypass was asked for. It should be rejected.

# What it does today

Neither, cleanly. It is rejected — but only by the **SSA backstop**, which
produces a bare unspanned message with no code:

```
t.sh: tainted variable z used as argument to command 'echo' without attest (SSA taint backstop)
```

`is_taint_source` honours `pe.taint_off`, so no SB5021/SB5020 is raised at the
IR level, and the SSA layer then rejects it anyway without knowing about `!`.
So the two layers disagree about what `!` means, and the user gets the worse
of the two diagnostics.

# Why it was left alone

Found while fixing the DD#61 source-into-collection leak (journal 430). The
fix made `is_taint_source` total, and mirroring `pexp_uses_tainted`'s
`taint_off` check was the choice that **left this behaviour exactly as it
was** — deliberately, so the fix did not prejudge DG's answer. Not honouring
`taint_off` would have silently picked reading 2 and turned the backstop into
an SB5021.

# Gate either way

Whichever reading DG picks, the outcome should not be the bare backstop:

- reading 1 → compiles clean; spec asserts `rc = 0` and the value
- reading 2 → a spanned SB5020/SB5021 naming the source, not the backstop

# RESOLVED — DG, 2026-09-27: reading 2

> "x=$(getenv A); y=$(getenv B); z=$x:!-$y:! ; the `!` should only apply to the
> read of the variable's stored value"

`!` is scoped to the read of its own subject, in both directions. Both gates
met: `${x:!-$(getenv HOME)}` is rejected, and with a **spanned SB5020** rather
than the bare backstop.

The question turned out to be sitting on a bug. The `!` of *every* `${x:!-…}`
was dropped in `parameter_expansion_to_value`, which wrapped the subject in
`SsaValue::Trusted` only when there was no operator — so even the
fully-vouched `${x:!-${y:!}}` failed to compile. Three validate sites had the
mirror-image fault (`taint_off` treated as covering the whole expansion), and
`source_is_int` needed to see through the new wrapper so `!` stayed a taint
annotation rather than a change of type.

Separately, DG declined to extend brace-less dispositions into a disposition's
operand (DD#90 rule 5), so his `$x:!-$y:!` spelling remains invalid and
`${x:!-${y:!}}` / `$x:!-${y:!}` are the working ones: *"I'd actually use
attest, so avoid over-indexing on taint-bypass ergonomics."* That behaviour is
now pinned by `var_modifiers.sh` vm.bl24-bl25.

DD#91 addendum (2026-09-27), specs `taint.sh` 91.24-91.33, journal 431.
