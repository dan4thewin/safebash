---
title: "DD#8 - Checked integer arithmetic as ordinary functions"
status: accepted
---

DG selected checked arithmetic functions on 2026-11-02, after comparing
them with a checked-number operator-overload design against the actual IR.
The choice keeps source integers as ordinary `std::int64_t` values.
No numeric wrapper, operator facade, or producer change is required.

## Operations and faults

The runtime supplies `add`, `sub`, `mul`, `neg`, `div`, `mod`, `pow`, `shl`,
and `shr`. Nested expressions can remain nested calls, for example
`mul(add(total, 1), 3)`, while functions, loops, and branches remain normal
C++ constructs.

Retain [native DD#25](../../notes/decisions/DD25-overflow-traps.md):

- Addition, subtraction, multiplication, negation, and exponentiation
  overflow are faults, not wrapping or saturation.
- Division/remainder by zero fail; `INT64_MIN / -1` overflows, whereas
  `INT64_MIN % -1` is zero.
- Shifts reject negative counts. Counts at least 64 produce zero on the
  left and zero/minus one by sign on the right. Right shift sign-fills;
  left shift discards bits without arithmetic-overflow faults.

Power retains the existing documented native result of zero for every
negative exponent (`sash-rt/src/lib.rs::sash_pow`), and one for exponent
zero, including `0 ** 0`. This is integer power, not a new floating-point
or rational operation.

An `ArithmeticError` holds the fault kind and a fixed diagnostic without
an allocating string payload. It is not a command verdict or
`CommandError`. Ignoring a returned integer does not suppress an operation
failure; the ordinary program body reports it and returns status 2 after
unwinding. [Experiment DD#5](DD05-planned-end-explicit-handling.md) still
governs END: arithmetic there requires explicit handling, with no
automatic catch, diagnostic, or abort policy.

## Implementation and emission boundary

The AI implements representable bounds checks with standard C++17 and
shifts with unsigned bits, avoiding signed overflow, invalid shift counts,
and implementation-defined unsigned-to-signed conversion. Power uses
checked multiplication by squaring, stopping before an unused final square.
This preserves the documented results while bounding the internal loop.

Comparisons and non-faulting bitwise operations can use ordinary C++.
Logical operators and ternaries must retain lazy regions. Operand effects
and failures can make evaluation order observable; nested helper arguments
do not impose source left-to-right order.

This bounded unit is arithmetic, not numeric parsing or text operations.
DD#77's accountable string-to-int conversion remains separate work.
Compile-time SB5077/SB5084 remain producer responsibilities. There is still
no IR reader, emitter, new native policy, or producer implementation.

The runtime and eighth paired report are implemented. See
[journal 015](../journal/015-checked-integer-arithmetic.md) for the
cross-host gates, arithmetic-only UBSan evidence, and isolated reversions.
