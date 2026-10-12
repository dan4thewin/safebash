---
title: DD#88 — Capture-condition: `if x=$(cmd)` binds stdout and branches on the command's status
status: accepted
---

Settled with DG (2026-09-02) and implemented the same session. A scoped,
shipped subset of the `multi-assignment-and-capture-trio` decision — enough to
make the long-standing `if ! out=$(cmd) { … }` idiom work — deferring the
general multi-value `return` / `(stdout, status, stderr)` trio to that task.

## The gap

Capturing a command's stdout **and** branching on its exit status had no
working form (see the `multi-assignment-and-capture-trio` motivation table):
`if ! out=$(cmd)` was SB4025; the "sequence it" workaround aborts on failure;
DD#55's `out=$(cmd) || …` idiom was itself SB4025. Root cause: DD#71 treats an
assignment as a value binding with "no meaningful exit status" — true for
`x=foo`, but `x=$(cmd)` carries the command's status, which the idiom needs.

## The decision

In a **boolean context** (an `if`/`while`/`until` condition, or the operand of
`!`), an assignment whose RHS is a single **external** command substitution —
`x=$(/path/cmd)` or a pipeline `x=$(a | b)` — is a **capture-condition**:

- the **condition truth value is the command's actual exit status** (`test =
  status == 0`); a nonzero exit does **not** abort (the status is consumed —
  DD#55's "binding the status" rule);
- the command's **stdout is captured into the target** (tainted external
  output — bless before use, DD#61);
- `$?` is available as the **first expression of the handling branch** (the
  existing DD#55 handling-position rule), so `if x=$(cmd) { … } else { echo $? }`
  reads the real failing code in the else branch;
- `!`, `else`, and `$?`-first-in-branch behave **identically to a plain command
  condition** — in particular the negated spelling `if ! x=$(cmd) { … }`
  replaces `$?` with the negated boolean (0/1), losing the magnitude, exactly
  like `if ! cmd`. To read the numeric code in-branch, use the non-negated
  `if x=$(cmd) { use $x } else { echo "failed: $?" }` form.

### Scope (DG-chosen, 2026-09-02)

- **RHS shape:** a single external `$(cmd)` OR a pipeline `$(a | b)` of external
  stages. Excluded (stay SB4025): a user-function capture `$(f)` (functions
  convey status via `return` — DD#74 predicates), an intrinsic `$(getenv X)`, a
  concatenation `foo$(cmd)`, a multi-statement `$(a; b)`, and any literal RHS.
- **Untyped only:** `if int n=$(cmd)` (typed declaration) is NOT a
  capture-condition — declarations stay SB4025.
- **`while`/`until` included:** the same form works as a loop condition.
- **stdout on failure:** left as-is (whatever partial bytes were captured); the
  target is meant to be used on success. A truly-unused captured target is a
  dead variable (SB5050) — if you do not need the stdout, use the
  non-capturing `if ! cmd { … }` form.
- **`$?` after:** no new rule — the existing DD#55 handling-position machinery
  applies (a capture-condition assignment provides `$?` like a command).

## Why this shape

The narrowed semantics are the union of two mechanisms that already exist, so
the implementation is mostly wiring, not new machinery:

1. `x=$(- cmd)` — captures stdout, suppresses the abort, stores the status.
2. `while read x` — an operation that binds a variable AND yields a condition
   truth value.

## Implementation

- **transform** (`transform/stmt_handlers.rs::try_capture_condition`): in
  `state.in_condition > 0`, a `CmdSubst` (single external) RHS lowers to a
  **suppressed** capture `Assign` + a `SetTestFromStatus` sigil; a
  `CmdSubstBody` that is a single `Pipeline` captures via `capture_body_into`
  with `state.suppressed` bumped (inner pipeline unchecked) + the sigil.
- **SSA**: new sigil `BuiltinCmd::SetTestFromStatus` (`__set_test_from_status`)
  — `test_result = ($? == 0)`, the sibling of `NegateTestResult`. A suppressed
  pipeline sets `test_result` itself; the sigil makes the single-command path
  uniform.
- **codegen**: native (`emit_set_test_from_status`) and c (`__test_result =
  (__exit_status == 0) ? 1 : 0`). Pipelines honor `state.suppressed` for their
  `checked` flag (`handle_regular_pipeline`).
- **validate** (`is_capture_condition_rhs`): the SB4025 carve-out. `validate`
  runs BEFORE command resolution, so the command head is still bare — the
  external test is `resolve::is_external_looking` (excludes intrinsics +
  builtins), NOT a post-resolution `/` check. It cannot consult the function
  table, so a bare user-function name also passes here; the **transform**
  rejects the residual `$(user_fn)` capture-condition (SB4025, via
  `reject_nonexternal_capture_condition`) since it has the function table. The
  capture-condition also registers as a `$?`-provider in the SB5012 state
  machine. The transform path is guarded on a `CommandSubstitution` source expr
  so an arithmetic mutation `x=1` inside `(( … ))` (also lowered with
  `in_condition` set) is untouched.
- **janet**: not implemented — janet emits from the IR (not the SSA sigils) and
  its `$(cmd)` capture is a stub. Positive-behavior specs are gated `native,c`;
  filed `janet-capture-condition`. The rejects (SB4025/SB5050/SB5020) are
  backend-independent and pass in all modes.

## SB codes

No new codes. Reuses SB4025 (non-capture value binding in boolean context),
SB5050 (dead captured target), SB5020 (tainted target used without bless),
SB5012 (`$?` outside a handling position).

The SB4025 message is context-aware: a plain value binding (`if x=hi`) gets the
"value binding — sequence it" advice; a `$(…)` capture that ISN'T a single
external command (a concatenation, or an intrinsic/builtin capture that can't
fail) gets a Group-B message that names the working form ("a capture-condition
branches on a single external command's exit status") rather than claiming the
feature is unsupported. A `$(user_fn)` capture gets its own transform message.

## Tests

`spec/capture_cond.sh` 88.1–88.18: success/failure branches, `$?`-in-else real
code, negated form, `$?`-first-in-then, pipeline + pipefail status, `while`
form, bare-external head (88.17), taint bless (SB5020), dead-var (SB5050), the
`!` vs `else` `$?`-magnitude contrast (88.15/16), and the exclusions with their
messages (user-fn / concat / intrinsic / builtin / literal → SB4025). journal
236.

## Deferred

The general feature — multiple-value `return`, multi-target binding, and the
external `(stdout, status, stderr)` trio — remains in
`multi-assignment-and-capture-trio`. This DD is the external-command
capture-condition subset only. A related open idea: extending presence-testing
to the getenv intrinsic (`getenv-presence-handling`) — either as a
capture-condition or via an optional getenv default argument.

## Addendum 2026-09-12 — `getenv` joins the capture-condition

The rule above is restricted to a single **external** command substitution,
on the grounds that a sash intrinsic "can't fail". [DD#101](DD101-getenv-lookup-status.md)
carves out one exception: `$(getenv NAME)` interrogates external state and can
report a not-found **result**, so it composes with both `x=$(getenv N) || …`
and `if ! x=$(getenv N) { … }`. All other intrinsics remain can't-fail, and
SB4025's advice text still applies to them.

## Addendum 2026-10-12 — redirects, and the capture that has no writer

DG, asked whether a redirect belongs in a capture-condition: *"so long as the
cmd's stdout remains connected to the caller (e.g., `cmd 2>&1 >f`) it should be
supported"* — and then, on seeing the rule written down: *"the idea that the
capture needs a writer even after redirects makes sense for all captures, not
just ones after `if`."*

So the rule is **not** a capture-condition rule. It is a rule about every
`$( … )`.

### The rule

Inside a substitution the body starts with fd 1 attached to the capture pipe.
A redirect list may move that pipe around. The capture is meaningful while
**some fd still holds it**, and nonsense otherwise — the value could only ever
be `""`. Simulated left to right, exactly as the kernel applies the list
(`ir::redirects_preserve_capture`):

| spelling | capture fed by | verdict |
|---|---|---|
| `$(cmd 2>/dev/null)` | stdout (fd 1 untouched) | legal |
| `$(cmd 2>&1 >f)` | **stderr** — fd 2 took the pipe before fd 1 left | legal |
| `$(cmd <in)` | stdout | legal |
| `$(cmd 3>&1)` | stdout, and fd 3 as well | legal |
| `$(cmd >f)` | nothing | **SB4075** |
| `$(cmd >f 2>&1)` | nothing — fd 2 dups the *file* | **SB4075** |
| `$(cmd &>f)` | nothing — the parser desugars this to `>f` | **SB4075** |
| `$(cmd 1>&2)` | nothing | **SB4075** |

A pipeline's capture is written by its **last stage**, so that is the stage
judged: `$(a >f | b)` is legal (`a` steals its own stdout from the next pipe,
not from us) and `$(a | b >f)` is not.

The check is permissive about what counts as a writer. **Any** top-level
statement of the body that could still write qualifies — `$(a >f; b)` is legal
because `b` feeds the capture — and a form whose writer is not plainly visible
(a compound, an assignment) counts as one. SB4075 only ever fires on a capture
that is unambiguously abandoned.

Fully decidable: `>&`/`<&` targets are literal fd numbers or the parser
already rejected them (SB4048), so there is no dynamic case.

### Why a new code rather than SB4025

SB4025 is "a value binding used where an exit status is consumed" — a
*boolean-context* fault. An abandoned capture is wrong in any position:
`a=$(cmd >f)` is the same mistake as `if a=$(cmd >f)`, and `echo $(cmd >f)` is
too. SB4075 is emitted from `validate_expr`'s `CommandSubstitution` arm, so it
fires wherever a substitution is written.

This is a **breaking change** against both bash and previous sash, which bound
`""` silently. It is the usual safebash trade: bash's answer is well-defined
and useless, and the program that wanted it is spelled `cmd >f`.

### What a redirect in a capture-condition now does

`try_capture_condition` previously matched two `SsaValue` shapes — a bare
`CmdSubst`, and a `CmdSubstBody` that `is_single_pipeline`. A redirected
command is neither (`single_command_subst` declines a redirected capture so the
redirect is lowered by the ordinary statement machinery), so it fell to
`_ => None`, whose only caller-side explanation is DD#88's user-function
rejection. **`if x=$(cmd 2>/dev/null)` was told that capturing a function's
output is not a condition, with no function anywhere in the program.**

Now a `CmdSubstBody` qualifies when its single statement peels to a pipeline or
to a command that is not a user function — the function-table check moves with
the relaxation, so `$(f 2>&1)` stays on DD#88's side of the line with its
unredirected twin. The body is lowered with `state.suppressed` bumped *and*
`uncheck_emitted_since` applied: only `handle_regular_pipeline` reads
`state.suppressed` at emission time, so a plain Command needs the post-pass to
be unchecked at all. Verified byte-identical to bash, including `$?` carrying
the real exit code into the `else` branch.

A second, smaller disagreement fixed alongside: `stmt_is_external_command`
peeled `-`/`!` but not `Redirected`, while the caller that peels a bare
command's redirect sat fifteen lines above it. `$(a | b 2>f)` drew a false
SB4025 on that account.

### Tests

`spec/capture_cond.sh` 88.19-88.28 (surviving redirects in `if`/`while`, the
`2>&1 >f` idiom with its file and its `$?`, fd 0, an unrelated fd, `-`
suppression, a pipeline stage, an earlier stage, and the user function that
still earns SB4025) and cap4075.1-8 (the rejections, in condition / assignment
/ argument position, both orders of the idiom, `&>`, a dup, a pipeline tail,
and the multi-statement body that is still fed).

`spec/commands.sh` csr.1-csr.9 predate this and asserted the old
bind-empty-silently behaviour. The six write-to-file cases were respelled
`2>&1 >f`, which exercises the identical codegen — same literal / computed
target, same open, same dup2 — while keeping the capture fed, so the silent
redirect-drop bug they were written for stays pinned.
