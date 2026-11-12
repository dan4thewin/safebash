title: DD#27 — Trampoline Function Model
status: implemented

1. Source function calls lower into inline control flow rather than a runtime
   call stack.
2. Direct self-tail calls become loops; non-tail and mutual recursion are
   compile-time errors on every call route.
3. Materialize new arguments in the current context before rebinding the next
   iteration's arguments. Collection-slot tail rebinding is not supported.
4. Every fresh private body lifetime is initialized; globals, arguments,
   collection slots and active enclosing bindings remain distinct (DD86).
5. An anonymous capture body cannot borrow its enclosing function's tail loop.

---

Functions are blocks ~~within the single Cranelift `_start` function, not
separate Cranelift functions.~~ within their owning executable root.
Call = branch, return = branch back.
DD128 also has separate private generated handler roots.
~~Positional params saved/restored around calls.~~ DD86's lexical identities
and fresh private lifetimes govern call storage. Enables inlining without
ABI overhead.

## Recursion (amended 2026-08-21)

Because calls are inlined (there is no runtime call stack / frame), an
unbounded call cycle cannot be inlined — it would inline forever and
overflow the *compiler's* own stack. So:

- **Non-tail recursion and all mutual recursion are rejected** at compile
  time with **SB5037** (a clean diagnostic, not a crash). Detection is an
  inline-stack in `VersionState.inlining`: a call to a function already on
  the stack is a recursive edge. Applies to all three inline sites
  (`inline_int_func`, `inline_function_call`, the pipeline-stage inline).

- **Direct self *tail*-recursion is supported** — it lowers to a **loop**.
  A self-call that is the producer of a `return` (`return F …`) is a tail
  call by definition (nothing runs after a return), so no separate
  tail-position analysis is needed. The first inline of `F` makes the body
  entry the loop header; a `return F newargs` rebinds `F`'s implicit `args`
  array (DD#75 §5) to `newargs` and branches back to the header instead of
  inlining again (`emit_tail_call`). Loop-carried `args` works with no manual
  phi: all SSA versions of a name share one Cranelift Variable, and
  Cranelift auto-merges at the join (see `codegen/setup.rs` `map_versioned_names`).
  - `return F …` is the ONLY tail form (required for `-> int`/predicate by
    DD#74, and used uniformly for void procedures too). A recursive call in
    any non-return position is non-tail → SB5037.
  - Idiomatic accumulator style: `fn go -> int { …; int n1=$(( n-1 )); int
    a1=$(( … )); return $(( (go n1 a1) )); }`. (Bind a local first; a nested
    `$(( ))` *directly* as a sexpr arg hits a separate pre-existing bug —
    `bug-arith-sexpr-nested-cmdsub-arg`.)
  - ~~The janet oracle keeps its own emit (real recursive `defn` calls); the
    tail transform is native/c (SSA). All three agree on results and on the
    SB5037 rejects.~~ DD104 replaces the in-tree oracle with an external seam.
  - Specs: `functions.sh` 27.1–27.5.

A general array **slice** (post-beta, `research-array-slice`) will subsume the
dropped `shift`; tail recursion + `for x in @args` cover the common iteration
needs today.

## Argument materialization repair (2026-10-18)

Both tail-call spellings use the shared argument lifter before rebinding
`args`. The producer form also uses the ordinary call-argument renderer, so
`return F @a` splats rather than joining `a` into one argument. Captures in
the new arguments execute in the current iteration, before its argument
array changes. `functions.sh` `tmpcap.19`-`tmpcap.20`.

The tail loop rebinds scalar `args`, not collection slots. A producer call
with `=collection` therefore takes the same SB5037 refusal as the existing
int-tail path; it cannot silently keep the old slots and treat the marker
as a string argument. `tmpcap.21`.

## Addendum (2026-10-29, batch-531 port F5): a capture is a call route

The recursion rule names three inline sites. A capture of a function,
`$(f …)`, inlines `f`'s body too, so it is a fourth route, and it skipped
the check. `$(f)` inside `f` overflowed the compiler's stack whether the
capture was assigned, passed as an argument, used as a capture condition or
suppressed (B57). A cycle that ran through a capture was caught only at a
later plain call, if there was one. Every route now goes through
`functions::reject_recursive_call`, which reports SB5037 at the statement
holding the call. A cycle `g → $(f) → g` is now reported at the capture,
the first edge that closes it.

An anonymous `$( … )` body inside `g` is not `g`. So a `return g …` written
in it is not `g`'s self tail call: the body no longer inherits `g`'s tail
loop, and the call is SB5037 like any other recursion. It used to branch to
`g`'s loop header from inside the open capture, and the program printed
nothing with status 0 (B59). The named-function case, B58, landed with F13.

Specs `functions.sh` rec.5-rec.9; journal 542.
