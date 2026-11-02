---
title: "DD#9 - Program inputs and strict integer conversion"
status: accepted
---

DG authorized this bounded runtime unit after checked arithmetic. Native
DD#60, DD#72, DD#77, DD#93, and DD#101 settle the source behavior; this record
does not introduce a new source policy or change the producer. The AI chose
explicit C argument-frame inputs, owned standard values, and one shared
`from_chars` parser as reversible implementation details.

## Program inputs

- Use ordinary C argc/argv explicitly, without a program-context object or
  global argv state. The count includes argv0.
- `argv(count, arguments, start)` returns an independent dense string array,
  including argv0 by default and reindexing a slice from zero. Wide slices
  are empty; negative indices and malformed metadata are `SetupError`.
- `arg` returns an owned string. Missing required arguments fail; an eager
  fallback applies only to absence, never to a present empty argument.
- `getenv` returns an owned optional string. Unset is `nullopt`, set-empty is
  present. **Lookup absence does not automatically abort** (DD#101); no
  checked-getenv/try-getenv split or default argument is introduced.
- Counted environment names are copied for the C interface; NUL is rejected,
  not truncated. Actual allocation failures escape rather than becoming
  absence. Source variables are not mirrored into the environment.

A C++ `(0, nullptr)` frame is a valid empty low-level value. Real sash
programs have argv0. Callers still owe a C frame of the stated extent and
lifetime; metadata guards cannot make an arbitrary invalid pointer safe.

## Numeric text

`parse_int` is allocation-free and locale-independent. `is_int`, `int_or`,
and `int_required` share it, including for owned scalar/cell reads.

Accepted text is a complete optional sign plus decimal digits or modern
`0x`/`0o`/`0b` digits, including uppercase prefixes. Runtime leading zeros
are decimal. Values must fit signed i64, including its asymmetric minimum.
Whitespace, partial input, missing digits, arbitrary `base#`, separators,
NUL, and out-of-range magnitudes are invalid.

`int_or` defaults on invalidity; valid zero remains zero. `int_required`
invokes its message only on invalidity, exactly once, and reuses the existing
`CommandError(2)` / `required value: ` renderer. Out-of-range input belongs
to this disposition, **not ArithmeticError**. Word-context emptiness
dispositions remain unchanged.

Failed conversion does not publish an assignment. Ordinary body unwinding
and owner END remain the existing lifecycle; there is no automatic END
handling. The source-facing default/taint/admission rules remain the
producer's responsibility.

## Bounded example and remaining work

The ninth `numeric_report` pair combines argv, getenv, scalar/cell
conversion, valid zero, a real charge function, checked multiplication,
and source-shaped conditions/traversal. Its regex uses DD#39's escaped
literal hyphen; source validity uses DD#93's `i?`, and lookup handling
uses DD#88's untyped capture-condition form.

No IR reader/emitter, numeric wrapper, general text/read family, native
compiler implementation, or producer change is included.
