title: DD#25 — decide overflow recovery model
status: done
depends: []
gates: [25.2, 25.3]

DD#25 says integers trap on overflow. DD#55 defines the error model
(implicit checks, `-` suppression, abort on failure).

Open question: is an integer overflow recoverable or non-recoverable?

**Recoverable (like command failure):** program aborts by default,
but could be suppressed with `-` or caught with `||`. Implies
overflow produces an exit-code-like result.

**Non-recoverable (like stack exhaustion):** program aborts
unconditionally. No suppression possible. Overflow means the
program's arithmetic model is broken — continuing is unsound.

Arguments for non-recoverable: overflow means a value left its
valid range. Any subsequent computation using that value is wrong.
Unlike a failed `mkdir` (which has a clear recovery path), there's
no meaningful way to "handle" an overflow — the number is simply
wrong.

Arguments for recoverable: the programmer might want to detect
and branch (`if (( x + y )) ...`), or use `-` to indicate they
accept wrapping semantics for a specific operation. Blanket abort
is inflexible.

Needs a decision before implementing DD#25.
