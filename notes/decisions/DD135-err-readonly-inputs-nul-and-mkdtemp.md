title: DD#135 - Readonly ERR inputs, NUL proofs and mkdtemp
status: implemented

1. Bindings inherited by ERR are implicitly readonly during its execution,
   including through same-process helpers, captures and collection aliases.
   Reject writes to their values or contents; ERR-owned working state is not
   frozen merely because it has global backing storage.
2. This is a boundary restriction, not a global `const` declaration, a new
   lexical scope or a public `readonly` operation. Immutability neither
   validates data nor resolves inherited taint.
3. `0? STRING` is true iff the counted contents contain a NUL byte. Empty
   strings give false; the storage terminator is not content. It is an
   ordinary unary test, with DD93's existing shorthand and predicate rules.
4. Every unary filename test returns false on interior NUL before filesystem
   lookup. `-nt`, `-ot` and `-ef` return false if either operand contains NUL,
   before lookup or missing-file comparison rules.
5. False `0?`, or true `i?`, `p?`, `g?`, `cpath?` or filename tests, establishes
   NUL-freedom for the tested value, not taint clearance.
   DD136 retires mandatory static ERR NUL proofs; runtime argv guards
   remain terminal, including inside an attempted call.
6. `$(mkdtemp [DIR])` atomically creates a new owner-only directory under
   `DIR`, or `/tmp` when omitted, retries name collisions and returns its
   absolute pathname directly as a clean, NUL-free string. `DIR` is an
   ordinary string operand evaluated once, with normal type, taint and
   operand-failure checks. Anchor relative parents to cwd after operand
   evaluation, preserving symlink and `..` semantics. The parent must exist;
   empty or NUL-bearing parents fail. Ignore `TMPDIR`; do not create missing
   parents or fall back to another directory.
7. Creation has a real operational status. An unhandled failure aborts
   before assignment; a legal capture-condition handles it, and an inner
   `-` may explicitly discard it. Handled recoverable failure supplies empty
   text and failure, never an old pathname or empty success. Fatal faults
   remain fatal.

---

DG requested this decision on 2026-11-14 after discussing the tour checker's
cleanup in ERR. DG described the restriction as an implicit `readonly` on
variables crossing into ERR, rather than changing every global declaration
to `const`. DG then selected `0?`'s polarity: **"Contains a NUL byte; false
for an empty string"**.

The counted-NUL predicate and filename guards are implemented in both
backends ([journal 639](../journal/639-counted-nul-and-filename-guards.md)).
Readonly ERR imports and predicate-derived ERR proofs are also implemented
([journal 640](../journal/640-readonly-err-and-local-nul-proofs.md)).
The direct `mkdtemp` constructor and tour cleanup migration complete the
package ([journal 641](../journal/641-mkdtemp-and-tour-cleanup.md)).
[DD136](DD136-err-call-attempts.md) retires only the
mandatory ERR NUL-proof component, together with SB5101. Readonly imports,
counted predicates, filename guards and the constructor remain implemented.
DG approved the optional-parent extension on 2026-11-26. It is implemented
in native and C on main;
[journal 678](../journal/678-mkdtemp-parent-directory.md) records delivery.
[Journal 679](../journal/679-slice-tour-and-acceptance.md) records its main
landing together with the accepted array-slice operation.

## Readonly at the ERR boundary

ERR receives the actual completed process-global state, not a snapshot of
main's hypothetical final state (DD128). Its inherited bindings are available
for reading but not writing for the duration of the handler. Main and EXIT
keep their ordinary mutability rules.

The restriction follows logical binding identity. Assignment, arithmetic
mutation, reader/output targets, append, element writes, clear and unset
are writes, including through a called helper or a forwarded A-E slot.
Overwriting an inherited binding before its first read does not make it a
new ERR-owned binding. A helper that writes such a global may remain legal
from main but cannot perform that write when called from ERR.

Working state introduced and used solely within ERR is not an import.
Fresh function/capture locals remain available under DD86. Conversely,
placing a declaration textually in ERR does not establish exclusive
ownership if ordinary execution also accesses its binding through a helper.
Main, EXIT and ERR still share their existing source scope; there is no new
shadowing, redeclaration or handler-local syntax.

This is a compile-time read-only view, not a runtime copy or memory freeze.
Compiler-managed status/latch updates and fresh private storage are not
source writes to an imported binding. Ordinary process-copy and child-handler
rules remain unchanged; a forked child is not another invocation of ERR.

Readonly supplies stability, not validation. DD128 still requires `:!` for
potentially tainted inherited reads. An ordinary overwrite was never taint
resolution; it is now also a forbidden write to an ERR import. Genuinely new
input may be acquired into ERR-owned mutable working state and attested
adjacently under the ordinary ownership, failure and resolution rules.
Value-preserving validation is not itself a write, but readonly grants no
late or inherited-taint attest.

No public `readonly` builtin, general collection-const syntax or arbitrary
mid-flow freezing operation is adopted here.

## NUL is a separate byte property

DD12/DD87 strings have counted bytes `[0..len)` and a storage terminator at
`[len]`. `0?` examines only the former. A NUL at any counted position makes
the result true, including a final counted NUL followed by the separate
storage terminator. An empty string is NUL-free, so `0?` gives false.

The ordinary full predicate for NUL-freedom is `[[ ! 0? $s ]]`. DD93's
shorthand remains literal: `attest s 0?` asserts that NUL **is present**;
it is not a NUL-free attest. General conditions, full attests and scalar/
collection unary shorthands follow their existing restrictions.

Filename predicates must never inspect a truncated prefix. A NUL-bearing
operand makes the whole predicate false before filesystem lookup. This
includes each operand of `-nt`, `-ot` and `-ef`, not only unary tests such
as `-d`. Treating an invalid operand as a missing file is insufficient:
`-nt` can otherwise be true when its first file exists and its second does
not. Both operand values are evaluated once; the byte check and lookup use
those values.

Thus a true filename test proves its operand or operands NUL-free.
Its false outcome proves no such thing, including when negation makes
that outcome satisfy a surrounding condition.
True `i?`, `p?`, `g?` and DD93's `cpath?` also exclude NUL by definition.
The `-t` descriptor test is not a filename test.

These facts neither clear taint nor promise anything about filesystem
stability, pathname ownership or a later operation's success. In particular,
successful `-d` is not protection against a filesystem change between the
test and a later command.

## Historical static proof at the ERR use

Before DD136, the ERR walk tracked `may_nul` independently of taint.
Literals and known NUL-free producers need no new predicate. For an
otherwise-unproved argv value, ask whether the necessary predicate outcome
holds at that use, on every path that reaches it.

The proof belongs to the actual value, not a name appearing in an earlier
test. Preserve instruction position, branch polarity, negation and
short-circuiting. Joins, loops, copies, defaults, collection selection and
same-process calls must not substitute another value or import a proof from
only one predecessor. Readonly imports remove their mutation invalidation
problem; they do not remove these path obligations. A successful main-path
test cannot stand in for a test after ERR's independent entry.

An explicit guard remains useful for an inherited, potentially tainted `work`:

```sash
if [[ -d ${work:!} ]] {
    if /usr/bin/rm -rf -- ${work:!} { : }
}
```

The first forced read permits inspection; the second permits passing the
data. The successful `-d` supplies only NUL-freedom for the stable value.
The command's own failure is explicitly handled. No inherited attest or
NUL-guard suppression is involved.

The original implementation kept its demand-driven query local to
`terminal::check_err`, using existing value facts as the fast path. DD136
removes that query rather than promoting it to a compiler-wide refinement
pass. An unguarded ERR argv operation may now compile but terminates ERR
if the runtime NUL guard fails. Other unconditional failures remain subject
to DD128/DD130 and their existing backlog.

## A generated temporary-directory value

`$(mkdtemp [DIR])` is a value-producing builtin, not stdout to be captured
through a pipe. The optional operand chooses the existing parent directory,
not a name template. It accepts ordinary string values, including computed
values and collection cells, without a literal-only restriction. Evaluate
it once before resolving the parent and attempting creation; an outer
condition or status discard does not catch its operand or helper failures.

A relative parent is anchored to the actual cwd after operand evaluation.
The returned pathname is absolute, so a later `cd` does not reinterpret it.
This is not lexical normalization or `realpath`: preserve ordinary pathname
resolution, including symlinks followed by `..`. Creation is exclusive and
atomic, with owner-only access. A fixed child-name prefix and generated
restricted-alphabet suffix are unchanged; a collision retries rather than
adopting an existing directory or symlink.

The parent is a normal checked read. Environment/input-derived values need
their ordinary legal attest or explicit forced read before use; the generated
suffix does not validate the parent. No `cpath?` or filesystem-ownership
proof is imposed or manufactured. The successful direct result introduces
no new taint and is NUL-free because all parent bytes are checked before
creation. Later fd reads still follow DD127. Assigning this clean result to
an already-tainted live binding does not resolve that binding's taint under
DD128.

An ordinary `work=$(mkdtemp)` implicitly checks creation before assignment.
The direct untyped form `if work=$(mkdtemp) { ... }` binds the new pathname
and consumes the operation's status. A handled recoverable creation error
binds empty text and supplies failure; it never reuses a previous pathname.
`work=$(- mkdtemp)` explicitly discards that status and retains the same
defined failure value. Assignment itself has no status.

The value occupies its own direct capture under DD111. No caller-selected
child-name template, arbitrary statement-position use or typed declaration
condition is added. Conditions, negation, status reads and discarded-result
restrictions retain DD55/DD88's existing rules. In ERR the destination must
also be writable under the import restriction. Under DD136, an unchecked
creation failure abandons an eligible attempt or terminates ERR.

Only omission selects `/tmp`; an explicitly empty parent is an error.
Missing/non-directory parents, interior NUL and failure to obtain a required
cwd are contained recoverable failures, using the same empty-value/status
contract as failed creation. No invalid parent reaches a truncated pathname
lookup. Do not create missing parents, consult `TMPDIR`, or choose another
directory after failure. Apply DD130 to the actual cause and post-failure
state. Heap and other genuinely fatal faults do not become an empty-string
result. Reserve the returned pathname before creating its directory;
publishing its return value needs no post-creation allocation. No particular
randomness source, suffix format, syscall wrapper, diagnostic code or runtime
ABI is selected by this decision.

This is independently useful without solving every existing file-I/O
handling gap. An initially empty binding populated only with this constructor
can supply NUL-free cleanup argv without external-output attestation or a
predicate-derived NUL proof.

## Implementation owners

- [NUL predicate and filename tests](../tasks/nul-predicate-and-filename-tests.md):
  complete; the predicate and all three shared filename boundaries enforce
  the counted-byte contract in both backends.
- [Readonly ERR inputs and NUL proofs](../tasks/err-readonly-inputs-and-nul-proofs.md):
  the original delivery is complete. SB5102 still rejects inherited logical
  writes; DD136 retires its localized SB5101 proof query.
- [mkdtemp value intrinsic](../tasks/mkdtemp-value-intrinsic.md):
  complete; native and C share exclusive creation and direct value/status
  handling, with independent clean provenance and NUL-free byte facts.

`HandlerEntry.inputs` currently contains every global, not an already-minimal
ERR import set. It cannot simply be relabeled readonly. Existing const-write
checks also assume scalar consts and cannot alone enforce collection contents.
These are implementation cautions, not new source scopes or chosen metadata.

The broader [failure classification](../tasks/runtime-failure-classification.md)
and [recoverable file handling](../tasks/recoverable-file-failures.md) remain
separate. The tour checker now uses the constructor and direct guarded
cleanup argv. Its sibling-file staging and atomic publication rename remain.
