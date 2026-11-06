# 575 - Orderly exit, explicit failure, and the unattended implementation

2026-11-06. DG asked whether normal completion should be able to report a
nonzero status, using grep's no-match result as the motivating distinction.
The source-level handler-function idea was set aside. Existing EXIT and ERR
blocks suffice when the operation, rather than its numeric status, selects
the termination cause.

DG then asked about an explicit failure operation and the implementation
size. The runtime already has separate orderly and failure entry points,
with generated handlers over actual globals, main-process ownership and
once-only progress. I assessed this as a contained integration change,
not another handler-architecture project. DG authorized recording it and
implementing it alongside the return-kind work in an unattended session
on a new branch.

## The decision

| Ordinary main-process operation | Handler | Default status |
|---|---|---|
| `exit [N]` | EXIT | 0 |
| `fail [N]` | ERR | 1 |

DG explicitly chose to allow `fail 0`: failure cleanup and the eventual
reported status are independent. Both builtins use the existing integer
operand rules and full i64 domain; only the final process boundary narrows
the status. An omitted handler never substitutes the other. Neither
operation returns a status that a surrounding `if`, `||` or `-` can catch
to resume execution.

An exit during either handler terminates directly, for any value. A fail
during EXIT enters ERR once; a fail during ERR terminates directly.
Children run neither parent handler. Ordinary uncaught failures and first
fatal faults still select ERR, including failures encountered while
executing EXIT or evaluating a termination operand.

EXIT's incoming-data problem is not new. It already checks actual zero-exit
entries, including helper calls. The new contract extends those existing
source and SSA checks to every explicit exit; it does not require every
global to be clean or invent an analysis of hypothetical exits. ERR keeps
its conservative inherited-input and explicit operational-failure rules.

I rewrote the operative DD128, DD55, DD56, DD89, DD24, DD121 and DD127
sections together with GUIDE, the manual and the contributor note.
The old numeric-routing rule remains history in git, not a conflicting
live alternative. `explicit-exit-and-fail` owns implementation.

## Unattended scope

Branch: `exit-fail-return-kinds`, created from
`647c53408d902bd79eceffe9ddd0d9c11a4b777c` on main.
The assigned new Copilot session is
`ed4c7981-c5a0-4e82-9832-9557dc269030`.
This recording precedes its launch and changes no compiler, runtime or spec.

The reversible staging choice is two coherent units:

1. Implement explicit exit/fail, so both terminal operations exist before
   return-path completeness needs to recognize them.
2. Implement the recorded return-kind contract and remove SB4017 only with
   the replacement guards. Include the already-settled integer/status
   pipeline boundary: removing the zero-result ban must not admit integer
   zero as a status-producing stage.

The second unit owns `enforce-function-return-kinds`,
`retire-return-ceremony-check` and `q-int-function-pipeline-status`.
Bare return and fall-through supply no result; explicit zero supplies a
real status or integer; compatible forwarding preserves kind through
redirects. Every non-void returning path supplies its kind. Self-tail edges
and process termination are not missing returns. Constant results remain
legal; no substitute ceremony check is wanted.

The existing anonymous-capture caller-status question, redefinition policy
and return-operand/disposition question stay separate. DD86 still requires
the body contract for anonymous captures; that is not permission to change
their caller-side status-consumption rules.

Both units require their normal focused evidence, ordinary predicted spec
gate, unbypassed hooks, journal/status updates, commits and pushes. The
worker must leave main and reference worktrees alone. Coverage stays paused;
DD129, unrelated backlog and the discarded handler-function interface are
out of scope. Implementation completion will be recorded by the worker,
not inferred from launching a session.
