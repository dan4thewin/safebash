title: Literal dollars belong to sash; Bash option flags belong to conversion
date: 2026-11-10

## DG's decision

The next question was `q-word-dollar-dash`. The compiler keeps `$-` as
literal text in unquoted and double-quoted words, while arithmetic rejects
a dollar that starts no variable. The AI proposed rejecting `$-` in words,
with retaining the current fallback or rejecting all non-expanding dollars
as alternatives.

DG chose a different boundary:

> reject $- only in sash-convert when bash emit options; keep literal for compiler

He then clarified the converter's treatment:

> reject is too strong a word - rather treat it as one of the unsupported bashisms

This keeps compiler behavior and assigns actual Bash option-flags
expansions to the converter's existing unsupported category. It is not
authorization to implement the converter change now.

## The existing category is enough

DD57 already defines manual conversion as a `# MANUAL:` annotation with
the unsupported construct passed through unchanged. Its `set` and `shopt`
handlers already use that category for unavailable shell options.
No new hard rejection, flag emulation or guessed literal replacement is
needed for `$-`.

The equivalent Bash spelling `${-}` follows the same converter rule.
Single-quoted, C-quoted and Bash-escaped literal occurrences do not expand
option flags, so a textual search for `$-` would classify the wrong thing.
Other conversions and annotations keep their existing contracts.

Native sash remains independent of that Bash interpretation. DD64 now
states the existing literal-dollar fallback explicitly. Other
non-expanding dollars remain literal; this is not the proposed broad
stray-dollar ban. DD65 defaults retain double-quoted-body interpolation.
Malformed recognized forms still diagnose, native `${-}` remains rejected,
DD72's arithmetic rule stays, and DD63's `s///` anchor convention stays.
Backslash does not acquire a `\$` escape.

## One coupled converter defect

Current source inspection confirmed both compiler fallback arms and the
converter's ordinary manual-annotation mechanism. Reading the braced
converter also exposed an empty-name path in its default rewrite.
A small probe using the existing `target/release/sash-convert` at the clean
recording baseline `95bff850` confirmed:

```text
input:  echo "[${-}]"
output: echo "[${:-}]"
```

The converter returned success, without stderr or a manual annotation.
`convert_expansion` treats `${-}` as `${NAME-default}` because the name
before the dash is empty and `chars().all(...)` accepts an empty sequence.
The same queued work must stop that rewrite and preserve the unsupported
parameter.

Unquoted/double-quoted `$-` also passed through without annotation.
Single-quoted, C-quoted and Bash-escaped literal controls were copied
without annotation. None of the converted programs was compiled or run;
those controls do not establish equivalence for unrelated conversions.
The five-line source and both output streams are retained as
`files/dollar-flags-20261110.{bash,out,err}` in the session.

## Recording and implementation boundary

DD57 records the converter category; DD64 records the native literal rule.
The existing task retains its slug, P3 priority and open status, changing
from question to work. It owns annotation and braced preservation, with
literal controls and before/reversion proof. B38's findings pointers now
distinguish the answered dollar-word question from its remaining work.

No compiler, converter, runtime, spec or build code changes here. The old
native/C tables were not rerun, and there is no new spec sweep or coverage
measurement. The only fresh behavior observation is the converter probe
above. Annotation enforcement and its tests remain queued.
