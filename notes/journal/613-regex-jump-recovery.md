title: Keep regex jump recovery on both native targets
date: 2026-11-10

## The choice

With x86_64 and AArch64 Linux selected, the remaining regex-recovery
question was whether to remove `setjmp`/`longjmp` or provide native jump
support for both architectures.

The AI recommended ordinary error returns through the shared regex
compiler: no regex-specific jump assembly, but a real parser-and-cleanup
refactor, also affecting literal-pattern validation. The alternative
retained the existing parser structure and required ABI-specific jump
support.

DG chose:

> jump support for both

DD129 now records that direction. Retain the Plan 9 regex compiler's
local jump recovery and provide correct x86_64/AArch64 support. No
particular implementation provider was selected; this does not adopt
musl/libc startup or reverse the no-libc runtime choice.

## What stays separate

The jump abandons a failed regex compilation, returns to its live recovery
point and preserves cleanup and the failure result for the caller. It
does not become general Rust unwinding or DD128's handler-transfer
mechanism. Buffer layout, saved registers/stack, C declarations and build
selection need to agree with the target ABI.

DD132 already requires invalid `r?` candidates to return quiet false,
while DD130 keeps resource faults fatal. Both recovery alternatives could
support that contract; neither automatically fixes error reporting.
Keeping jumps does not authorize an unconditional diagnostic or process
exit for invalid candidates.

The engine/dialect, result kinds and provisional scratch/regex rules
remain unchanged. No regex cache, public ABI, compiler-host matrix,
cross-compilation interface or migration sequence was chosen here.
The native recovery choice is accepted independently of the provisional
DD131/DD132 trial.

## Source evidence, not an implementation result

Narrow source inspection at `49abbbf1` confirmed:

- `sash-rt/csrc/regcomp.c:108-115` holds the recovery buffer and jumps
  after reporting an error; 693-694 establishes the recovery point,
  and 805-810 cleans up and returns a failed compilation.
- Several parser helpers rely on the error path not returning to them,
  rather than already propagating a failure through their callers.
- `sash/build.rs:19-27` builds the same compiler for literal-pattern
  validation; `sash-rt/build.rs:19-31` still builds `setjmp.S`
  unconditionally.
- Runtime `regerror9` at `sash-rt/src/lib.rs:2467-2487` prints, and the
  inspected `sash_regex_match` caller at 2516-2519 aborts on a null
  compiled program. That is not quiet candidate validation.

The existing x86-only assembly inventory was not a new AArch64 build
or execution result.

## Recording and ownership

`cross-compile-and-setjmp-on-mac` retains the jump port and target-specific
proof, including normal/recovery paths, ABI state and failed-then-valid
compilation. `q-glob-semantics` retains the integrated trial's quiet
validation and fatal-fault distinction. The P1 private-rustix repair
remains separate and open.

GUIDE, the syscall and dynamic-regex records, the trial owner and status
now agree; the old ordinary-return recommendation is explicitly not
selected. All implementation remains queued. No product/spec/build
source changed, no runtime or cross-target probe was run, and no full
sweep or fresh coverage accompanies this recording.
