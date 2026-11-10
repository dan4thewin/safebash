title: Useful defaults for :? abort messages
date: 2026-11-10

## The decision

The next user-facing question was R3 in `q-f2-residuals`: a bare `${s:?}`
can abort with only the program prefix, identifying neither the subject
nor the reason. DD77 had left the default message unspecified.

The AI proposed a generated default naming the subject and its applicable
failure whenever the literal message is omitted or explicitly empty,
while preserving nonempty custom literal messages. Requiring a nonempty
message at every failsafe was the alternative.

DG accepted:

> yes to better default

The rule covers scalar/cell word checks, arithmetic string conversion and
array splats. Their triggers remain different: empty word/collection
versus a string that is not a valid representable integer. A non-numeric
or out-of-range integer candidate must not be described as merely empty.
This changes diagnostics, not successful values, abort status, handler
routing, message interpolation or subject/index evaluation.

Exact wording, subject rendering and implementation remain deferred.
This is policy/backlog recording, not an instruction to start compiler work.

## Evidence and ownership

The historical F2 probe was not rerun. Source inspection at `ba49c22c`
confirms that `sash_require_value` prints only the prefix/newline when the
message is empty; arithmetic conversion calls that path, and splat
disposition has a separate equivalent path. Native/C emission passes
the message through. The new task retains those entry-point locations.

DD77 records the accepted default, with DD65/DD69 and GUIDE aligned.
The AI filed P3 `default-require-diagnostics` as its implementation owner,
rather than closing the mixed F2 record or reopening the completed
interpolation/other-abort tasks. R3 is settled; the work remains open.
Its gates require native/C behavior, meaningful runtime-message assertions
and negative/reversion controls without predetermining the mechanism.

No compiler, runtime, spec, manifest, lockfile or build-command change
accompanies this recording. No new runtime probe, full sweep or coverage
measurement was performed.
