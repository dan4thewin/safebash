# Checked integer arithmetic and the eighth pair

**2026-11-02.** DG directed continuation with checked arithmetic functions,
after comparing them with operator overloads against the actual exported
IR. [DD#8](../decisions/DD08-checked-integer-arithmetic.md) records that
choice. The AI implemented the runtime helpers and harmless
[C++](../../sash-cpp/examples/arithmetic_report.cpp)/
[sash](../../sash-cpp/examples/arithmetic_report.sash) arithmetic report.

## Implementation and boundaries

Source integers remain ordinary `std::int64_t` values. `add`, `sub`, `mul`,
`neg`, `div`, `mod`, `pow`, `shl`, and `shr` retain native DD#25; its shift
rules were already settled, not a new experiment choice. In particular,
`INT64_MIN / -1` faults, `INT64_MIN % -1` is zero, negative shift counts
fault even for zero operands, and wide right shifts saturate to the sign.
Left shifts discard bits without arithmetic-overflow faults.

The AI chose representable bounds checks instead of compiler intrinsics,
and unsigned-bit shifts with a mathematical signed conversion instead of
implementation-defined casts or negative signed shifts. Checked power
uses exponentiation by squaring, avoiding an unused final square that
would reject representable answers such as `(-2) ** 63`. Positive exponent
work is bounded; negative exponents retain the documented native zero,
and exponent zero returns one, including `0 ** 0`.

`ArithmeticError` carries only a fault kind and fixed diagnostic, not a
command verdict or allocating string payload. This is a payload claim,
not a claim that the C++ exception ABI never allocates. The body boundary
catches this specific new type and reports status 2 after unwinding.
Assignment targets remain unchanged on failure, ignored function results
do not suppress arithmetic, and failed conditions/arguments do not become
false or call their bodies.

END remains governed by [DD#5](../decisions/DD05-planned-end-explicit-handling.md).
Explicit arithmetic catches work; unhandled END arithmetic escapes the
C++ caller without automatic status, diagnostic, or suppression policy.
No inherited checkedness mode or generic exception catch was added.

The report retains an int-returning subtotal function, counting for,
ordinary if/while, nested helper calls, and a lazy ternary. Its independent
golden has ten lines. Comparisons, safe bitwise operations, and lazy
regions remain ordinary C++; future emission must sequence operands when
effects or failures make their order observable.

## Evidence

The predicted count was 44 fixture groups, up three from 41, with eight
C++ goldens, up one from seven. The final tree produced exactly those
counts on GCC, Clang, supported Linux GCC ASan, actual Apple Clang/libc++,
and Apple ASan. Both ordinary Linux compilers also passed the existing
fault gates and unchanged 169-case command/capture catalog.
The new C++ report's exact stdout and empty runtime stderr were checked
on every host/compiler configuration.

The arithmetic selector, covering all three new groups, and its report
also passed GCC UBSan. Two separate deliberate canaries failed with the
intended signed-overflow and negative-left-shift diagnostics under the
same flags. This is arithmetic-only UBSan evidence, not a full-runtime
UBSan claim; the existing
[host compatibility task](../tasks/sanitizer-host-compatibility.md)
remains open. No sanitizer category or resource fixture was disabled.

Six independently linked source mutations each returned status 1 for
the intended assertion, not a timeout, compile failure, or hardware trap:

| Mutation | Assertion that failed |
|---|---|
| Add wraps through unsigned bits instead of faulting | Addition above INT64_MAX must fault |
| MIN modulo minus one raises overflow | INT64_MIN modulo minus one is not a fault |
| Power squares after its final bit | A negative power may reach INT64_MIN exactly |
| Left shift masks its count | Left shift by the width does not mask the count |
| Right shift omits sign fill | Signed right shift floors rather than truncates |
| Body boundary omits ArithmeticError handling | Ignoring a function result must not suppress its arithmetic failure |

Unmodified production controls passed before and after. All 44 project
source-file hashes remained unchanged through these checks and matched
the authorized isolated Mac workspace before and after its gates.
Mutation sources and logs remain in session artifacts; temporary mutation
executables, objects, sanitizer canary executable, and transfer archives
were removed. Formatting and whitespace checks pass.

## Reference corrections and changing producer

The new source draft incorrectly used an int collection's word-loop
binder as a bare arithmetic integer. The real compiler rejected it with
SB4030: that header supplies strings, not a numeric counting loop.
Declaring the binder int did not change the header's values. The report
now uses a genuine counting loop. A local `total` also collided with the
later global `total` under SB5049; it was renamed `sum`. These were example
mistakes, not native compiler bugs.

During reference validation the supplied compiler changed beneath its
symlink and rejected the unchanged seventh pair's `amend_options =A =A`
with SB5099. DG's 2026-11-02 DD#100 amendment in the active reference
checkout requires distinct collection identities and access through
passed/forwarded slots instead of overlapping global names anywhere in
the call chain. The option pair now forwards one slot; its seventeen-line
golden is unchanged. Ordinary C++ references and generic alias-safety
fixtures remain. [Journal 014](014-collection-argument-runtime.md)
preserves the earlier duplicate-slot demonstration and its evidence.

Another checksum comparison detected a further live rebuild. A bounded
fresh-checksum/copy/checksum comparison pinned dereferenced copies of the
compiler and companion archive before the final reference gate:

| Pinned input | SHA-256 |
|---|---|
| Supplied `sash` compiler | `947362d34d5cb68c78e1d9d9a0c33a21d0203e7dd78caaa4ffc4dd3f24d2aee0` |
| Supplied `libsash_rt.a` | `d26debdce3f691f0880d5a2745bdf2e1f7c875815c25a8e4fc676bba48f10dfa` |

All eight sash pairs compile and match their independent goldens against
those copies; the eighth's exact stdout and empty runtime stderr were
also checked separately. The active checkout's newer DD#100 is recorded
as reference compatibility, not imported by another rebase or edits to
this branch's native compiler.

## Scope

There is no numeric wrapper, operator facade, integer parser, IR reader,
emitter, or producer implementation. SB5077/SB5084 remain producer
responsibilities. Numeric ingress/conversion, remaining text/regex/read
families, general redirects, public wait, and forked-C++/background
ownership remain on the [runtime roadmap](../tasks/cpp-transpiler-runtime-before-ir.md).
No native source, decision, journal, status, or spec changed.
The protected JSON producer draft remains unchanged and uncommitted,
and was excluded from source-only Mac transfers.
