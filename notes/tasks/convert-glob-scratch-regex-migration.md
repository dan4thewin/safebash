title: Adapt Bash conversion to accepted glob, scratch and regex contracts
status: open
priority: P2
kind: work
depends: []
gates:
  - "Select and record the converter migration policy before implementation; do not infer automatic reset, replacement, renaming or annotation from native-language adoption"
  - "Sequential converted mapfile/readarray fills preserve replacement intent or explicitly require review; they must not silently reuse earlier records through accumulating native read"
  - "Account for aliases, later reads, loop declarations, failure publication and taint when translating replacement fills"
  - "Handle mutable underscore bindings, glob/rglob reserved names, immutable runtime-pattern proofs and regex text/boolean consumption without silently changing source meaning"
  - "Preserve already-correct conversions, including loop-local declarations that reset each iteration; use DD57's review boundary for cases without a sound automatic translation"
  - "Real converter-to-CLI native/C cases pin each changed translation, with before/reversion evidence and fresh spec ids for changed contracts"

## Ownership

DG accepted DD131/DD132 and requested landing on main on 2026-11-18.
The [integrated trial](q-glob-semantics.md) is complete; its measured
converter compatibility work continues here. Acceptance of the native
language does not claim converter compatibility or select a translation
algorithm. No converter implementation is part of journal 661.

## Verified compatibility failure

[Journal 654](../journal/654-glob-scratch-regex-evaluation.md) compared
original main `6c7b899f` with trial `76376c4e` using matched native/C
compiler/runtime pairs. The converter source was unchanged.

Two converted `mapfile -t records` operations into the same ordinary array,
with their existing attest annotations reviewed, both compile and exit
successfully. Reading `records[0]` after each fill printed `first`, `second`
before the native accumulation change and `first`, `first` afterwards.
The second fill appends rather than replaces. A loop-local `array records`
declaration is a preservation control: it resets each iteration and prints
the intended two records.

The same evaluation measured refusals for mutable `_label` reassignment
(SB5103), functions named `glob`/`rglob` (SB5092), and mutable or unproved
runtime regexes (SB5106). Text-only regex calls also need explicit Boolean
discard. Hand-reviewed renaming, fresh results and validated candidates
work, but are not a general converter repair.

## Decision boundary

Consider automatic translation where equivalence can be established and
explicit review where it cannot. In particular, clear-before-fill can lose
the old destination on failure; a private fill followed by replacement must
preserve aliases and taint obligations. Blanket underscore renaming needs
identity-aware handling of every use and collision. A new pattern binding
must validate its final immutable value, not manufacture proof through `:!`.
Present consequential policy choices to DG when undertaking this work.

The native accumulation/scratch contract, regex engine and result kinds are
settled. This task does not reopen them or own
[regex allocation/reuse](dd39-precompile-literal-regex.md).
