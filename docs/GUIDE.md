# safebash — A Guide to the Architecture

*For anyone continuing or contributing to this project.*

This document explains how safebash works and why it is designed the
way it is. It is written for someone with a working knowledge of
compilers and Rust, but not necessarily deep expertise in either.

**A note on authorship:** This guide was written by an AI (Claude,
Anthropic) working under the direction of DG, the human creator of
safebash. The AI wrote the prose and the code; DG directed the
effort and made design decisions. See `notes/journal/001-prologue.md`
for a fuller account of how this collaboration works.

---

## The Chain of Trust

safebash exists to make shell scripts participate in a chain of trust:

```
Author writes safe-subset script
    → sash validates and compiles to native binary
        → Binary is signed in the build pipeline
            → System policy (IPE / fs-verity) verifies before execution
```

Every architectural decision serves this chain.

## Safety Model

Three dimensions:

1. **Language-level safety.** The safe subset eliminates constructs
   that make Bash difficult to analyze: no eval, no indirect expansion,
   no implicit coercions. The compiler handles bash idioms silently
   (backticks → `$()`, `[ ]` → `[[ ]]`, unquoted `$var` is safe because
   sash has no word splitting or globbing).

2. **Data-level safety.** Taint quarantine (DD#24, DD#42–44). All
   externally-sourced data (env vars, positional params, read input,
   external command output) is tainted. Tainted data cannot be used
   at all until validated via `attest VAR EXPR`. Environment variables
   are accessed via `$(getenv NAME)`, positional params via `$(argv N)`.
   No "variables just appearing" — every value has a visible origin.
   `$(getenv NAME)` also reports a *lookup* status (DD#101) — success when
   the name is present, including when it is set to the empty string;
   failure when it is unset — so `home=$(getenv HOME) || home=/root`
   distinguishes absent from empty. It is the one intrinsic admitted to
   the DD#88 capture-condition; the rest are pure functions of their
   arguments and cannot fail.

3. **Supply-chain safety.** Compiled artifacts are signed. The system
   refuses to run binaries whose signature or integrity check fails.

Safety does NOT mean sandboxing. External commands (`rm`, `mount`,
etc.) are legitimate operations. Safety means the script's intent is
visible, its data boundaries are explicit, and its binary is
tamper-evident.

---

## Two-Tool Model (DD#57)

The toolchain splits concerns between two binaries:

- **sash** — the compiler. Accepts only sash-native syntax. Validates,
  type-checks, and compiles to static ELF. Never transforms source.
- **sash-convert** — the converter. Accepts Bash and produces sash-native
  source with mechanical coercions, MANUAL annotations for constructs
  needing human review, and CHECK/ATTEST hints.

The split enforces a clean boundary: sash-convert is a best-effort
transformation tool; sash is a sound compiler. A script that compiles
with sash is safe by construction. A script that only passes through
sash-convert still needs human review.

Typical workflow:
```
bash script.bash → sash-convert → script.sh (review) → sash → binary
```

---

## Repository Structure

```
safebash/
├── sash/                  — The compiler (Rust crate)
│   ├── src/
│   │   ├── main.rs        — 4-line wrapper calling sash::cli_main()
│   │   ├── lib.rs         — CLI entry, pipeline driver
│   │   ├── parse/         — hand-written lexer + recursive descent → AST
│   │   │   ├── logos_tokens.rs — the logos `Tok` enum
│   │   │   ├── lexer.rs   — Scanner (peek/peek_n/next)
│   │   │   ├── parser.rs  — recursive descent → Program
│   │   │   ├── expr.rs    — words, strings, expansions, subscripts
│   │   │   ├── arith.rs   — `(( ))` / `$(( ))`, DD#58 sexpr calls
│   │   │   └── error.rs   — parse_error helpers
│   │   ├── ir.rs          — IR definitions (the AST the parser yields)
│   │   ├── validate.rs    — structural rules + taint + positionals + splat
│   │   ├── type_check.rs  — Type checking (int vs string)
│   │   ├── resolve.rs     — DD#23 command resolution
│   │   ├── transform/     — IR → SSA (entry, statements, control_flow,
│   │   │                    expressions, functions, pipelines, handlers)
│   │   ├── ssa.rs         — SSA representation and printing
│   │   ├── ssa_taint.rs   — SSA-level taint verification
│   │   ├── codegen/       — Cranelift code generation (entry, values,
│   │   │                    commands, arithmetic, dispatch, redirects,
│   │   │                    setup, types, handlers)
│   │   ├── emit_c.rs      — C oracle backend
│   │   ├── emit_ir.rs     — IR capture (inert Janet data)
│   │   ├── regex_validate.rs — DD#39/#63 regex validation
│   │   └── diagnostic.rs  — Error reporting
├── sash-convert/          — Bash → sash-native converter
│   └── src/
│       ├── main.rs        — CLI entry point
│       ├── convert.rs     — tree-sitter-bash CST → sash-native source
│       └── known_externals.rs — Auto-attest patterns
├── sash-rt/               — Runtime library (#![no_std], rustix, no libc).
│                            A SEPARATE cargo workspace — build it with
│                            `(cd sash-rt && cargo build --release)`.
├── sash-expand/           — Expansion utilities
├── docs/                  — GUIDE, TIMELINE, sash.1
├── notes/                 — iwe knowledge graph (journal, decisions, tasks)
├── audit/                 — Adversarial review results
│   └── run-1/             — 5 models, 3 passes, remediation done
├── spec/                  — Spec test suite (41 files; results in spec.db)
├── contrib/               — Vendored: plan9port-regexp, freertos-heap4
├── suites/                — brush/oils test runners
├── exp/                   — Git submodule (safebash-exp, optional)
└── mkfile                 — Plan 9 mk build system (build, test, clean)
```

**The compiler has no tree-sitter dependency.** `sash-convert` uses *stock*
`tree-sitter-bash 0.25`, and it uses it on *bash* input — the language it
reads, not the language it writes. The old `~/src/tree-sitter-bash-safebash`
fork drove a pipeline that commit `1c0e64a` deleted, and is no longer part
of the build.

---

## Compilation Pipeline

```
 Bash source (.sh)
       │
  [1]  │  Parse
       ▼
 parse/ (logos lexer + recursive descent) → Program (AST)
       │
  [2]  │  Validate
       ▼
 validate.rs → diagnostics (SB4001–SB50xx)
       │
 [2a]  │  Resolve
       ▼
 resolve.rs → absolute command heads; bindings.rs → lexical identities
       │
  [3]  │  Taint
       ▼
 validate.rs::check_taint → per-definition taint (SB4020/SB5020)
       │
  [4]  │  Lower
       ▼
 parse/ already yielded the IR (ir.rs); coercions are applied
 by the parser and the transform — there is no separate lowering pass
       │
  [5]  │  Type check
       ▼
 type_check.rs → checked scalar/element types and collection kinds
       │
  [6]  │  Transform
       ▼
 transform/ → SSA (ssa.rs): versioning, phi nodes, basic blocks, CFG
       │
  [7]  │  Fold
       ▼
 Constant folding in compile_to_object
       │
  [8]  │  Codegen
       ▼
 codegen/ → Cranelift IR → object code (.o)
       │     Declares extern functions from sash-rt
       │     Embeds .sash_source/.sash_ir/.sash_manifest
       │
  [9]  │  Link
       ▼
 lib.rs invokes ld → object + libsash_rt.a → static ELF binary
```

### Phase 1: Parse

A hand-written lexer (`parse/logos_tokens.rs` + `parse/lexer.rs`) and
recursive-descent parser (`parse/parser.rs`) produce the AST directly.
There is no CST and no tree-sitter dependency in the compiler — commit
`1c0e64a` removed both. (`sash-convert` still uses stock
`tree-sitter-bash` on *bash* input; that is a separate crate.)

Some text is lexed twice: a statement inside a quoted capture, a
subscript index, a sed field and a `=~` chain are re-lexed by a
sub-scanner. Each sub-scanner is given its text's place in the program
(`lexer::Origin`), so a diagnostic inside it reports its own line and
column, and the re-lex site reports the sub-scanner's fatal ahead of
whatever the sub-parse returned (DD#114, DD#115). Text the arithmetic
lexer rebuilds from tokens maps back to no source position, so it
reports the enclosing expression's (`Origin::Within`).

**Accepted grammar, implementation pending:** `cmd && { a; b }` is
exactly `if cmd { a; b }`, and `cmd || { a; b }` is exactly
`if ! cmd { a; b }`. These are conditional bodies, not general command
groups; standalone groups and their marked/stage/return forms are
retired. Scope, body checks, control transfers and status/latch flow
match the corresponding `if`. DD95 still forbids mixed `&&`/`||` lists.
DD55/DD68/DD70 record the rule; `retire-command-groups` owns enforcement.

`if`, `while`, `until` and `for` are statements with no value or status
result (DD68). Reject consuming a compound's result, including through
negation, return or a logical continuation; an otherwise legal unconsumed
final action remains a statement. Inner condition handling, explicit
function returns, capture output and pipeline statuses keep their own
contracts. Enforcement is pending in `q-compound-as-condition`.

### Phase 2: Validate

`validate.rs` walks the AST once. (The old `rules/` directory and its
`Rule` trait are gone — commit `1c0e64a` folded them, the standalone
taint pass, and the CST itself into this single module.)

| Check | Codes | What it catches |
|-------|-------|-----------------|
| `check_dynexec` | SB4001 | `eval`, `source`, `.`, `command`, `coproc` |
| `check_heredoc` | SB5002 | heredocs / here-strings |
| `check_command_head` | SB5011 | an unknown name in command position |
| `normalize_command_head` | SB5096, SB4074 | a quoted *constant* command name said any way but the one accepted spelling, or an empty one (both parse time) |
| `check_exec_target` | SB5011, SB5095 | an `exec` target that is not a runnable program |
| `check_loop_control_context` | SB5081, SB5097 | a `break`/`continue` that would be a silent no-op — no enclosing loop, or a loop count that cannot mean what it says (zero, non-literal, non-numeric, or more than one) |
| `check_attest_predicate_code` | SB5098 | an attest predicate that would run code: any `$( … )` (`echo`, the value intrinsics, `getenv` and the `${…:s///}` sugar included), `$(<file)`, or `$(( … ))` at any depth, or a call in a DD#93 Part D value; a shape rule, so it runs under `--no-taint` |
| `check_taint` | SB5020 | a tainted value used without `attest` |

There are **no reserved words.** A bash builtin sash does not implement
(`export`, `declare`, `typeset`, `readonly`, `select`, `shift`, …) is
simply an unknown command and gets SB5011 like any other unresolvable
name — so any of them can be defined as a user function and called.
(A parser table used to intercept six of them with bespoke diagnostics;
DG retired it on 2026-09-12 because it both reserved the names and made
the unknown-command story non-uniform.) `eval` / `source` / `.` are the
exception: they are *rejected*, not merely unknown, and cannot be
redefined away.

### Between validation and semantic checks: resolve names

After structural validation, `resolve.rs` resolves external command heads
(DD#23), then `bindings.rs` gives each lexical local one internal identity
(DD#86). Function-name prefixes distinguish sibling locals; captures get
distinct child identities while retaining access to enclosing bindings.
Globals stay shared, and control-flow bodies stay in their existing scope.
The resolver rewrites name operands, not literal values or map keys.

Typing, semantic validation and SSA lowering all consume the same resolved
AST. A source-name table keeps diagnostics and runtime attest labels in the
user's spelling. The original AST/source remain the artifact input:
`--emit-ir`, embedded IR and embedded source do not expose compiler names.

The first semantic check is `check_const_writes` (SB4044). On the resolved
AST each local already carries its scope's qualified name, so one
program-wide set of consts is exact, and every way a statement binds a name
is checked against it, wherever the const is declared (DD#86). Lowering only
asserts the result: binding a const twice in `VersionState::define` is an
ICE. A merge phi binds through `define_merge` instead, because each version
it joins was checked where it was written.

Compiler temporaries use the same reserved-name set, through
`VersionState::reserve_temp_name`: a private-looking spelling cannot alias a
source global or another call's live result. The `__` namespace remains legal
source, not a compiler reservation (DD#125).

Function arguments are materialized before installing the callee's `args`
and collection slots. Plain calls, captures, pipeline stages and tail calls
share the argument lifter; nested calls retain their caller's positionals,
and splats remain splats. Pipeline arguments still execute inside their
existing forked function stage, not in the parent (DD#81, DD#27).

Every executable body entry initializes its newly owned private locals
(DD#86): empty strings/collections and integer zero. Named bodies, templates
and anonymous captures share one lowering hook; self-tail back edges run
those assignments again. Canonical lexical ownership preserves active
enclosing locals, globals, arguments and slots. The checker's `CheckedTypes`
supplies collection kinds as well as scalar/element types, including implicit
reader targets; lowering does not infer them again.

A `-` discards the result of the one command or call it prefixes, and
nothing else (DD#55 2026-10-30). The transform keeps a suppression depth on
`VersionState`: `lower_suppressed_stmt` raises it, and each status check
reads it as the check is emitted. Operand and argument lifting, the operands
of a `(( ))` test, and every inlined body run at depth zero, so a capture
among a suppressed command's operands, or a failure inside a called
function, is checked as it would be without the `-`. A suppressed capture
`$(- cmd …)` passes a flag to its own command instead of raising the depth.
Legal suppressed captures retain their normal value and type; structural
validation, taint and status-availability rules still apply (DD#55).

DD55/DD71 also require the prefix to be adjacent to that command (DG,
2026-11-08): `- x=$(cmd)` is rejected, not shorthand for `x=$(- cmd)`.
Append and cell bindings follow the same rule. A leading `-` cannot
discard a later `&&`/`||` operand's result, just as a pipeline-head `-`
cannot discard a later stage's status. `reject-nonadjacent-suppression`
owns pending assignment refusal, list binding/classification and the
remaining arithmetic-write operand barrier. Capture conditions retain
their handling contract.

DD55 also rejects consuming a status explicitly discarded by `-` (DG,
2026-11-08). This covers conditions, tested `&&`/`||` clauses, capture
conditions and DD74 status return producers. All-suppressed pipelines
cannot supply such a status; partial pipelines still supply their
unsuppressed-stage aggregate. A final `a || - b` at statement level,
discard followed by a fresh `return 0`, and testing a capture's text
rather than its discarded status remain legal. Ordinary handling is not
source discard merely because it disables an implicit abort.
`reject-discarded-status-consumers` owns pending enforcement.

DD55 also rejects a value-intrinsic capture such as `$(- toupper abc)`
when it has no separate status to discard (DG, 2026-11-08). The capture
already consumes its value; write `$(toupper abc)`. This includes
status-less integer intrinsics, but preserves `getenv`'s lookup status
and ordinary explicit value discard such as `- wait`. Redirects and
possible fatal faults do not change the result kind.
`reject-statusless-intrinsic-suppression` owns pending enforcement.

DD55 rejects source `-` on `return`, `exit`, `fail`, `break` and
`continue` too (DG, 2026-11-08), including through redirects. Remove the
prefix, not the transfer. Non-void calls remain eligible for result
discard; return forwarding, operand checks and DD128 terminal routing
remain unchanged.
`reject-control-transfer-suppression` owns pending enforcement. The no-op
`:` is separate from control transfers.

DD31/DD71 make assignments and arithmetic mutations no-result writes
(DG, 2026-11-08), not successful status producers. Reject `- (( n += 1 ))`;
keep the unprefixed mutation. An arithmetic comparison must be consumed,
not stand alone, even with `-`; bare `(( 0 ))`/`(( n + 1 ))` are rejected
too. A write cannot supply another expression's operand value, and
parentheses cannot create a result. Writes establish no fresh
`$?`/`$(status)` permission. Existing operand checks, capture-condition
results and fatal faults remain. `q-arith-test-statement-status` now owns
pending arithmetic enforcement and migration of the old status idiom.

DD70 separates legal discard from useful statements (DG, 2026-11-08).
Reject known result-only computation whose answer is ignored, including
`- [[ $s == yes ]]` and the statement-level `[[ $s == yes ]] || :`.
Writes, I/O, observable operand/body effects and uncertainty prevent this
rejection. A silently discarded recoverable failure or incidental OOM
does not make the computation useful. Explicit `:` remains the intentional
no-op, not an effect that excuses an enclosing test. Constant-returning
functions remain legal; only a known pointless invocation is rejected.
`reject-pointless-statements` owns pending enforcement and diagnostic
advice. Existing typing, placement, taint and failure rules remain.

### Phase 3: Taint

Flow-sensitive taint analysis (DD#24, DD#42–44), tracking names whose
live bindings remain unresolved.

**Resolution is adjacent (DD#121, DD#128).** A successful legal attest, or
a clean whole-subject replacement written directly on its failed path,
resolves outstanding taint. Ordinary overwrites, unset, clean loop binding,
buffering and self-copy through `:!` do not. Permission is subject-specific
and cannot enter helpers, captures or children. A fresh acquisition has its
own adjacency; a fresh private lifetime does not inherit a previous call's
taint. R2 still applies when no later read needs the subject.

**Global/slot summaries (DD#127).** Each function is analysed independently of
its callers: an empty-entry walk finds its own generators, and one walk per
global or symbolic slot A-E finds incoming-taint flows and conditional
diagnostics. Summaries are built bottom-up and applied at the actual calls,
not against the final top-level state. Returns contribute to the exit set;
self-tail back edges grow the entry to a fixpoint. Inherited-taint reads and
attests (SB5088) are reported at the call with the original body location.

A single-use function may leave its own taint for the caller to attest
immediately after the call. A shared function must attest it internally;
SB5067 names the last global/slot source and explains why the function is
shared. Ordinary calls propagate legal resolutions as well as taint.
Captures and arithmetic/value calls conservatively union their effects:
resolution cannot hide an earlier read or clear a binding when the call is
skipped.
Background children and forked stages are checked on isolated states; the
final parent reader retains its effects.

**Collection identities are mapped at each call (DD#100).** The formal
slots map positionally to actual collections, including forwarded and
function-local ones. A passed global's unused identity flow is omitted so
it cannot undo cleanup through its slot. An in-body fill/attest now clears
the actual collection at a statement call (B115); a read before that
cleanup still produces a conditional diagnostic. Received taint is never
freshly adjacent at body entry. The old `SlotInfo` fixpoints are removed;
slot usage remains a separate arity/dead-variable analysis.

**Output taint follows the read boundary (DD#127).** Every kernel-backed
capture and fd/pipe read introduces taint, including builtin-produced pipes.
Compiler-owned buffer captures do not; an internally captured and attested
value can therefore be echoed into a clean outer buffer. Environment and
program arguments remain sources, and a buffer copy cannot bypass a tainted
value's use check. There is no descriptor-routing or `clean_pipe` exemption.
Source analysis shares buffer eligibility with lowering; the SSA backstop
classifies the actual emitted capture path.

**Shared output is validated inside its producer (DD#127).** SB5100 rejects
an outside attest of tainted output owned by a shared function, naming that
function and its call sites. Capturing it and using `:!` is still legal.
Internal capture/attest/echo can export a clean compiler buffer; a subsequent
kernel read reintroduces taint and is not exempted by the writer's validation.
Existing per-function summaries carry stdout owners, true origin flows and
symbolic stdin/name attest obligations. Origin flows are separate from boolean
taint flows so a genuine new acquisition cannot inherit the old value's owner.
Ordinary clean writes preserve unresolved taint and owners together.
Named readers and slot forwarding use those summaries, not caller-specific
walks. A parent's private reader pipe is not the stdin of its calls or captures.

**Loop paths retain complete state (DD#121).** Break exits and continue
back edges carry both taint and ownership. Heads stabilize before the
diagnostic walk; speculative passes keep nesting depth without exporting
effects. While exits retain post-condition state. For words are checked at
the stable head, and the binder changes only an entered iteration, not the
zero-trip path. These transfers feed the existing independent summaries.

**SSA checks are process-local (DD#127).** The backstop checks the main
process and each background job or lifted pipeline stage separately.
Children inherit independent copies of the completed parent view; their
definitions cannot taint or clean the parent or a sibling. The shared CFG
walk follows same-process edges, then the verifier recursively checks
spawned children. EXIT and ERR are independent main-process roots.

Unbound function templates remain for lowering diagnostics and metadata,
but never enter those executable views. Lowering a definition restores
current variable versions without rewinding their allocation counters:
defining a function does not execute its assignments.

Definitions now converge before use checks, including forward phis and
values derived from them. Ordinary definitions retain the actual active
predecessor's taint; legal failed-branch replacement can remove that link,
but never bless a tainted new value. Unreachable phi predecessors are
excluded. Cell reads, splats, iteration and map-fill operands carry their
reaching collection version, not a future version selected by name.

Handler entry is checked separately (DD128): EXIT requires its ordinary
uses clean at each applicable explicit-exit, fallthrough or signal entry; ERR
conservatively treats any potentially tainted incoming binding as unresolved.
Source summaries retain transient acquisitions, including inside called
bodies and EXIT. Incoming-origin obligations reject late attests even when
one branch reacquires the value. The SSA backstop independently checks
incoming versions, mapped slots and fresh acquisitions.

Unlowered redirect operands still use the completed bare-name view, and
children still inherit the completed parent view rather
than a precise fork-time snapshot. DD#127 records these boundaries,
counted-capture lowering and conservative value/capture precision questions.

**Non-aliasing is already enforced (DD#100).** Collection arguments must
have distinct identities, and none may overlap a global named in the
callee or its transitive callees. Reads, writes, attests, metadata and
identity forwarding all count, including conditional paths and captures.
`check_collection_args` uses resolved identities and the shared call graph
to report SB5099 at the call, with a conflicting body location. This
language check runs even with `--no-taint`; it is the prerequisite for
independent slot summaries, not a taint-safety guarantee on its own.

**Taint sources:** `$(argv N)` (program arguments), `read`
targets, `$(external cmd)`, environment variables via `$(getenv NAME)`.
Function-local `$1`–`$N`/`$@` are always clean — passing a tainted
value as a function argument is a read, which is a compile error.
Top-level `$1`–`$N` are compile errors (DD#46: use `$(argv N)`).

**Clean sources:** literals, arithmetic from clean operands, `$0`,
`$#`, `$(status)`, `$(getpid)`, `$(getcpid)` and `$(waitstatus)`. A clean
expression keeps a clean destination clean; it cannot by itself resolve a
previously tainted binding. The punctuation spellings `$?`/`$$`/`$!` are input
sugar for the last three (DD#75); the keyword form is canonical and is
what `sash-convert` emits. As an arithmetic operand the spelling is the
`(name)` sexpr — `$(( (status) + 1 ))` — not the command substitution,
which is SB4020 there.

**Auto-attest:** `known_externals.rs` provides auto-attest patterns
for 26 environment variables and 25 commands.

**`attest VAR EXPR`** — evaluates EXPR (tainted VAR readable inside
the expression). If truthy, marks VAR clean. If falsy:
- Bare `attest` → implicit abort on failure
- `if attest` → false branch taken

```bash
port=$(argv 1) || { echo "usage: $0 port"; exit 1; }
attest port [[ $port =~ ^[0-9]+$ ]]
echo $port   # clean — attested above
```

**The rule:** a tainted binding can be read only in its eligible attest
predicate or through `:!`; ordinary unresolved reads receive SB5020.

**Containers (DD#61):** a tainted value cannot enter an array/map via a
literal, `+=` append, or `a[i]=` subscript (SB5021); tainted arrays come
only from bulk reads (`read -a`, `mapfile`). Collections are attested
per-element with attest-all — `attest x in a [[ … ]]` (indexed, binds each
value) or `attest k v in m [[ … ]]` (map, binds key and value). A failing
element aborts; an empty collection passes vacuously. Scalar `attest` on
a collection is a type error.

### Phase 4: Lower — no longer a phase

There is no lowering pass. The recursive-descent parser produces `ir.rs`
types directly, so the CST→IR step commit `1c0e64a` removed had nothing
left to do.

The coercions that pass used to perform now happen in two places:

- **In the parser**, where a surface spelling has a single IR form —
  `declare` → typed declarations (int, array, map), `let` → `(( ))`,
  backticks → `$()`, `[ ]` → `[[ ]]`.
- **In the transform**, where the choice depends on types or on
  resolution — glob→regex for a pattern, int/string conversion, command
  resolution (DD#23, `resolve.rs`).

The step numbering below is kept as-is so that phase names in the journal
and in diagnostics still line up.

### Phase 5: Type Check

`type_check.rs` infers int vs string for each lexical binding and returns
the checked type map directly to lowering. There is no second inference
walker. Cross-type assignment without conversion is a compile-time error.

- `int x=5` — an int variable
- `fn name -> int { return 5; }` — an int-returning function
- `$#`, `$(status)` (sugar: `$?`), `$((...))` — intrinsically int

String-valued dispositions require explicit arithmetic conversion at
integer sinks, including return operands: `return $(( ${s:-7} ))`, not
`return ${s:-7}` (DD89, DG's 2026-11-06 ruling). Already-integer dispositions
keep their type. The return-only conversion exception and the separate
integer `:?` typing bug remain pending implementation in
`bug-return-status-string-operand` and `bug-expr-type-int-disposition`;
the accepted rule is not yet fully enforced.

Top-level and cleanup statements establish global types before function
bodies are checked. Forward references and loop-carried copies converge
without a fixed iteration cap; inferred seeds are distinct from bindings
encountered in the current pass. Only the stable pass reports errors.
Captures and cleanup bodies receive every check, including predicate checks.

Each function has **one inferred signature** (DD#81/DD#100). Scalar
fixed/variadic arity remains inferred from `args` usage. Collection-argument
count, array/map kind and string/int element type must agree at every call,
including forwarded slots. A call-graph fixpoint resolves collection types
before checking conflicts; every checking visitor uses the same function
environment. Definitions are program-top-level only (SB5071); main and
cleanup calls all participate, using shared global collection facts.
Inlining does not make a function
polymorphic.

### Phase 6: Transform

`transform/` converts IR → SSA. Variable versioning, phi nodes at
merge points, basic blocks with explicit terminators, control flow
graph construction. Function inlining. Arithmetic evaluation. A
diagnostic raised while lowering reports at the statement being lowered
(`VersionState::stmt_span`).

**Evaluation order: accepted, implementation pending.** DD86 requires
left-to-right evaluation across argument lists and within individual words
and reached operands (DG, 2026-11-06; journal 580). Earlier reads retain
their values before later captures or value calls mutate state. Existing
lazy branches and process/body boundaries remain intact. The compiler's
captures-first behavior is queued for repair in `q-capture-hoisting-order`,
including DD55's first-expression `$?` handling.

**Arguments before attached redirects: accepted, implementation pending.**
DG chose the same timing for standalone commands and pipeline stages
(2026-11-07; DD86, journal 585). Evaluate and retain arguments first, then
evaluate each redirect target and apply its redirect in order, then run
the command. Pipeline plumbing remains earlier, and stage arguments stay
in the child; active enclosing redirects and capture routing retain their
scopes (DD122). A later redirect failure cannot undo earlier argument
effects (DD130). `q-stage-capture-redirect-order` owns the phase-order
repair; the current stage path still installs attached redirects first.

#### Functions, inlining, and recursion (DD#27)

**One definition per function name: accepted, enforcement pending.**
DG's 2026-11-06 rule rejects every second source definition, including an
identical body or unchanged signature/result kind (DD#86). Names remain
global and hoisted, so all calls refer to the same body. There is no
replacement or overload selection by source position.
`reject-duplicate-function-definitions` owns implementation (journal 582).

User functions are **inlined**; their calls do not create runtime stack
frames. A function cannot in general call itself — inlining would not
terminate. `SB5037` rejects those calls. Generated EXIT/ERR callbacks are
separate implementation functions, not exceptions to this source rule.

**One shape is allowed: a direct self tail-call**, where the recursive call
is the producer of a `return`. That compiles to a **loop**, so it runs in
O(1) stack. Write recursion in accumulator form:

```sash
fn fact -> int {
  int n=$(($1)); int acc=$(($2))
  if (( n <= 1 )); then return $(( acc )); fi
  int n1=$(( n - 1 )); int a1=$(( n * acc ))
  return $(( (fact n1 a1) ))     # tail self-call → compiled to a loop
}
int r=$(( (fact 5 1) ))          # 120
```

Everything else is `SB5037`:

| shape | result |
|---|---|
| direct self tail-call — `return $(( (f …) ))` | compiled to a loop |
| non-tail self-call — `return $(( n * (f m) ))` | **SB5037** |
| mutual recursion — `f` calls `g` calls `f` | **SB5037** |

A suppressed capture-return, `return $(- f ...)`, is SB4076 in an int f:
`$(f)` binds the int that f returns, so `-` has nothing to discard (DD#55
addendum 2026-10-30). The separately lowered suppressed loop that journal 530
built for it is gone (journal 547).

The non-tail case is the one worth naming explicitly, because it is what a
direct translation of the textbook factorial produces: the multiply happens
*after* the call returns, so there is nothing to turn into a loop.

**Every function is int, status or void** (DD#55). Under the accepted
2026-11-06 contract, bare return and fall-through supply no result and are
valid only for void functions. Status and integer functions supply their
own kind on every returning path. `return 0` supplies a status in an
unannotated function or an integer in an `-> int` function, never void.
Compatible forwarding preserves the callee's kind through redirects;
self-tail calls are loop edges, not exits.

Constant results are legal, including zero. The old int-only SB4017 check
is retired without a general constant-result check replacing it. The
kind/completeness changes and check removal are implemented in shared
`lower_body` checking, for named functions and anonymous capture bodies
alike (journal 577). They reuse the canonical function classifier.

A call must handle a status as a condition (`if`, `while`, the left side of `&&`
or `||`) or discard it with `-` (`- f`, `$(- f)`). Anything else is SB4016,
as an int function's unbound answer already was. `-` with nothing to discard
is SB4076: on a void function, or in `$(- f)` on an int f, whose `$(f)`
binds the int.

**Anonymous captures may not explicitly return a status** (DD#55/DD#86,
DG, 2026-11-06). This includes `return 0` and status-producing forwarding,
with or without redirects. Bare return, fall-through and compatible void
forwarding remain legal. Give a status-returning body a name; `$(- f)`
then captures its stdout while explicitly discarding its status.
No new anonymous-result consumer or capture-condition form is introduced.
The rule is accepted; enforcement is pending in
`reject-anonymous-capture-status-returns` (journal 584).

**Anonymous captures do not inherit caller status** (DD#55/DD#86,
DG, 2026-11-07). Entry starts without `$?` permission; handling inside the
body can establish its own. Save a caller status at a legal outer handling
position before using it as ordinary data in the capture. DD126's fresh
function-latch boundary applies too. Direct `$(status)`, `$(getcpid)` and
`$(waitstatus)` are value expressions in their surrounding context, not
new executable bodies. This changes neither lexical binding access nor
global effects. Enforcement and the existing per-body flow bugs remain
in `bug-sb5012-sb5094-nested-body-flow` (journal 586).

**No process-replacing `exec` inside stdout captures** (DD#86, DG,
2026-11-08). The source command `exec PROGRAM ...` is rejected directly
and through called functions; wrappers, conditional paths and child bodies
do not make it legal. Ordinary external commands still fork/exec their
children, and fd-only `exec >file` retains its existing semantics.
No subshell is introduced, and DD58's direct integer-value calls keep
their uniform spelling-independent contract. The rule is accepted;
enforcement remains pending in `q-exec-in-capture` (journal 587).

**Integer captures stay integer-valued through redirects** (DD#58, DG,
2026-11-06). `$(f 2>&1)` and `$(f >file)` consume f's returned integer;
stdout is a permitted side effect, following those redirects.
`$(- f 2>&1)` must reject just like `$(- f)`, rather than select stdout.
A void wrapper such as `fn output_f { - f }` can discard the integer and
expose stdout through `$(output_f)`, subject to normal taint/failure rules.
The connected-writer check (SB4075) applies to stdout captures, not integer
values. This rule is accepted but its redirect-preserving enforcement is
pending in `bug-int-capture-redirect-switches-kind` (journal 583).

**Integer values are not statuses** (DD#58, DG 2026-11-05). Direct int
function stages, including `- f | ...`, and `return int_fn` in a non-int
function require SB4030; discard does not repair a type mismatch. Integer
captures, int-to-int return forwarding and in-process value discard remain
valid. Both boundaries are enforced: `q-int-function-pipeline-status` covers
stages, and `enforce-function-return-kinds` covers return-producer
compatibility (journal 577).

**Single external background jobs exec directly** (DD#55, DG,
2026-11-08). The accepted model removes the implicit sash wrapper:
`cmd &` gives `$(getcpid)` / `$!` the command's actual pid, so signalling
it does not target an intermediate process. The automatic completion
diagnostic goes; the command's own stderr and exec-startup diagnostics
remain. The parent can still reap the outcome with `wait` /
`$(waitstatus)`, without automatic failure propagation.
Backgrounding a user function explicitly retains a checked wrapper.
Functions, builtins, pipelines and multi-command jobs keep their existing
body/process semantics. DG's 2026-11-09 follow-up rejects the redundant
direct-job spelling `exec cmd &`: write `cmd &`. Otherwise legal `exec`
inside a function or multi-command body remains meaningful and unchanged.
The direct-exec default and prefix refusal must ship together;
implementation is pending in `background-external-direct-exec`
(journals 594 and 595).

### Phases 7–9: Fold, Codegen, Link

Constant folding, then `codegen/` lowers SSA to Cranelift IR. Declares
extern functions from `sash-rt`. `codegen/entry.rs::embed_artifacts`
embeds the source, the IR capture and the manifest as `.sash_source`,
`.sash_ir` and `.sash_manifest` ELF note sections (DD#45, DD#109). Finally,
`lib.rs` invokes `ld` to link the object with `libsash_rt.a` into a fully
static ELF binary.

Source globals have actual process-static storage: writable native data
symbols and file-scope C static objects (DD#128, journal 567). The shared
lexical inventory and checked types supply `SsaProgram::globals`; function
locals and compiler temporaries remain local. All SSA versions of a global
use the same backing cells. Attestation changes trust, not storage, so an
abort before the attest still observes the last completed write.

A global whose declaration has not executed starts at its meaningful zero:
integer zero, an empty string, or its own empty collection header. Collection
backing is allocated on first mutation, not at startup. The empty headers
are distinct, so mutating one cannot change another; ordinary fork isolation
still applies. Empty strings need a nonnull pointer because collection cells
use a null pointer to tag integers. Initial string globals and stored
`readline` EOF values preserve that distinction; EOF's verdict remains false.

---

## IR (Intermediate Representation)

Defined in `ir.rs`.

### Statement types (StmtKind)

Assignment, Declaration, Command, Pipeline, List (&&/||), If, While,
For, Case (the `[[ ]]` dispatch form, DD#66), Function, Redirected,
Subshell, Group, ArithmeticCommand.

Subshell exists in the IR for bash-mode input; the transform coerces it
into sash primitives (DD#38).

### Expression types (ExprKind)

Literal, SimpleExpansion, DoubleQuoted, SingleQuoted,
CommandSubstitution, ArithmeticExpansion, Concat,
ParameterExpansion, Array, AssocArray, Subscript, Test.

---

## SSA (Static Single Assignment)

Defined in `ssa.rs`.

- **SsaProgram** — functions, each containing `Vec<BasicBlock>`, and global storage metadata
- **SsaInst** — Assign, Command, Pipeline, Test, ReadLine,
  ReadLineMulti, CaptureStart/End, Export, Attest, Trap, etc.
- **SsaValue** — Literal, Var, ParamExpansion, ArithExpr,
  ArithBinOp, Concat, DoubleQuoted, etc.
- **VarRef** — `{ name, version }`
- **PhiNode** — `{ target, sources: Vec<(BlockId, VarRef)> }`
- **Terminator** — Goto, Branch, Return, Loop

Version state uses a high-water mark counter that never rolls back,
preventing version collisions across if/else arms.

Elidable command substitution: when `$(func)` calls a known function
whose body is all `echo` with literal args, the output becomes a
compile-time constant.

---

## Runtime (sash-rt)

`sash-rt/src/lib.rs` (~3750 lines) — a `#![no_std]` Rust crate.
Uses `rustix` for raw Linux syscalls. No libc dependency.

**Accepted direction, implementation pending (DD#129):** retain public
rustix APIs and centralize unsupported Linux calls in a project-owned Rust
syscall shim. The current private `rustix::runtime` dependency still needs
removal; the fresh-clone build defect is not fixed by recording the choice.
macOS support is assigned to a separate C++ transpiler, which can also
cover exotic targets. This does not retire the existing `--emit-c` backend
or claim complete native AArch64 support.

### Entry point

`_start` — inline assembly for x86_64 and aarch64. Sets up argc,
argv, envp from the stack, calls `sash_main`.

### Memory

`sash_alloc` — bump allocator.

### String operations

`sash_concat`, `sash_itoa`, `sash_atoi`, `sash_strlen`,
`sash_substring`, `sash_strip_prefix`, `sash_strip_suffix`,
`sash_toupper`, `sash_tolower`, `sash_table_join`, `sash_regex_match`,
`sash_regex_match_into`, `sash_regex_replace`, `sash_fnmatch`.

All strings are `(ptr, len)` pairs with a null terminator at
`ptr[len]`. This gives length-bounded operations for safety and
null-terminated C interop for `execve` without copying (DD#12).

Both backends use `sash_table_join` for collection joins. It sizes the live
values and separators before allocating one terminated result, preserving
insertion order, integer-cell formatting and embedded NUL bytes. There is
no 4 KiB join limit; insufficient heap follows the fatal allocation
failure path (DD#107/DD#128), rather than truncating or overrunning a
scratch buffer.

### Regex

Plan 9 regex engine, vendored from `contrib/plan9port-regexp`. A
builtin family (`match` / `sub` / `gsub`) takes ordinary string args
(DD#63); the `${var:s/re/str/flags}` / `[[ =~ ]]` syntaxes are thin
sugar. **Captures** (DD#63): `match S P ARR` fills `ARR` with
`[whole, g1..gN]` (bash-parity: `N` = the pattern's group count, surfaced
via the engine's `Reprog.nsub`; a non-participating group is empty),
clears the array on entry, and returns the boolean. `sub`/`gsub`
replacements expand `\0`–`\9` backrefs and `\\`; any other `\<char>` is a
compile error (SB4033).

**Two boundaries are worth knowing, because they are the whole dialect's
sharp edges.** Full treatment in `sash.1` §REGULAR EXPRESSIONS.

The engine is **byte-oriented**, not character-oriented: `.` consumes one
byte, so `é` needs `..`. That is not a design choice so much as a
consequence of `sash-rt/csrc/lib9.h`, whose `chartorune` is an eight-line
ASCII-only stub — the single most user-visible divergence from bash, and
the one least visible in the source.

A regex operation examines its subject only **up to the first interior
NUL**; bytes past it are neither matched nor replaced, and the operation
reports success. Everything else the engine cannot do is *rejected*
(SB5023); this and the unsupported POSIX bracket classes (`[[:alpha:]]`,
read as an ordinary bracket expression) are the only two that fail
quietly. Documented deliberately rather than fixed — see DD#63's addendum
and `notes/tasks/limitation-regex-stops-at-interior-nul.md`.

Counted repetition (`{n}`, `{n,m}`, `{n,}`) caps each bound at **32**;
33 or more is rejected. The cap is a memory-safety bound, not an
arbitrary limit: `regcomp.c` sizes its instruction array from the pattern,
and an unbounded count overran it (journal 290).

### I/O

`sash_echo`, `sash_write`, `sash_read_line`,
`sash_heredoc_redirect`.

Both backends use the same redirect metadata and runtime save/restore
primitives. The transform reserves the script's literal fd operands across
functions, captures and handlers; both backends register that set before running
source code. Runtime-owned scope copies, backups, pipes and capture memfds
avoid it. Private copies and pipe endpoints are close-on-exec; explicitly
opened script descriptors remain inheritable. Backups also exclude every fd
named by their redirect list, including closed dup sources. Scoped commands restore an
originally closed descriptor by closing it again; successful `exec` commits
the redirects and closes its backups instead. Failed opens or dups take the
shared setup-failure path, so the command never runs on the wrong fds.

### Failure classes (accepted, implementation pending)

DD130 distinguishes recoverable failures, where known-state continuation
is useful, from fatal faults, where stopping is preferable. This is not a
setup-versus-execution distinction. Path-specific file errors are
recoverable and may be caught or suppressed; an unchecked one still aborts.
A failed redirect must skip its command and restore descriptor state,
never send output to the original destination or invent a successful value.

Fork failure joins heap exhaustion and arithmetic faults in the fatal
class: conditions, negation and suppression cannot resume the failed
operation. Resource errors from other I/O paths require explicit
cause/state classification. Function result kinds, no-reach into helpers
or operand captures, and DD128's process/handler ownership remain unchanged.

The work is queued in `runtime-failure-classification`,
`bug-fork-failure-reporting-by-form` and `recoverable-file-failures`.
The runtime paths described below are implemented; their remaining
failure-class inconsistencies are not claimed fixed by this decision.

### Process management

`sash_exec` — execve with absolute path (no PATH walk; DD#23).

Ordinary and lastpipe/capturing pipelines use one stage builder, retaining
per-stage suppression, negation and inlined functions. Both backends check
pipe/fork results before consuming outputs or recording pids. Construction
failure closes successful pipes and waits for already-started stages, retrying
interrupted waits; it never force-kills them or adds a kill timeout (DD#55).
Capturing consumers have an explicit skip edge on failure, rather than reading
an invalid fd or treating failure as EOF. Child fd wiring and redirects share
the checked setup machinery; a failed setup never executes the stage body.

A single unredirected external `$(command)` lowers to a one-stage pipeline
with a private value-capture target. Arguments run in the parent before
pipe/fork, stdout remains pipe-backed, and the parent drains it before
waiting. It shares pipeline construction checks and caught/suppressed/failure
routing, without publishing or invalidating an enclosing lastpipe reader.

Parent-reader pipelines stream before waiting (DD#122). Each site owns a
stack-resident runtime record containing its fd, pending children and enclosing
record. Read instructions carry that site's negative encoding; stdin is never
rebound. Binding is lexical at reader construction, suspended across calls,
captures, redirects and background children, rather than a broad SSA rewrite.
Both `readline` and a bulk `read` in a bounded loop body follow that rule.

The shared `transform::reader_cleanup` pass places `PipelineFinish` on CFG
edges leaving a reader region, inner first; conditional exits get bridge blocks
with phi predecessors retargeted. Finishing closes before reaping and applies
the existing suppression/pipefail fold. Nonlocal transfers retain their saved
status after successful cleanup. Runtime terminal cleanup closes all active
readers before reaping, preserving the triggering status; forked children close
inherited reader fds without adopting the parent's children. Public `wait`
results are cached for a later owning-pipeline fold. Native and C share this
lifecycle, including early scalar completion and tail-call back-edges.

Function and multi-statement command-substitution bodies have **two** capture
paths, chosen per site at compile time by `capture_purity`:

* `sash_capture_start_buf` / `sash_capture_end_buf` — the fast path. The body
  is *pure*: nothing in it forks onto this capture's stdout and nothing
  rebinds fd 1. Output is collected in a userspace buffer, fd 1 is never
  redirected, and the capture costs **zero syscalls**. `sash_write` diverts
  at the `fd == 1` boundary.
* `sash_capture_start` / `sash_capture_end` — the general path. fd 1 is
  pointed at a **memfd** (not a pipe: a captured body runs in-process with
  nobody draining, so a pipe self-deadlocks past 64 KiB). Used whenever the
  body contains a pipeline, a redirect or a bare external.

A body may contain an external and still be pure, so long as the external's
output arrives through an *inner* `$( )` — that child gets a pipe of its own.
Direct externals disqualify a site. The current implementation also still
classifies `exec CMD` as impure (journal 546), but DD86 now rejects that
source operation in stdout-capture bodies; enforcement is pending in
`q-exec-in-capture`. The recording does not change either capture transport.
The predicate is conservative: every pipeline or redirect selects the
general path, even when the intended output comes only from builtins. A value
call counts: an int function called as `$(f)`, `(f)` or a bare `f` in
arithmetic runs its body where the call stands, so a fork in that body
disqualifies the site too. A capture whose arguments make such a call takes
the body path, so the call runs inside it (journal 545).

The runtime keeps a **heap-backed linked stack** of capture frames, because
an impure capture can nest inside a pure one: the memfd path pushes a
transparent frame that suspends buffer diversion for its duration. Depth is
static: functions are inlined, and the one recursive shape sash accepts — a
direct self tail-call — becomes a loop rather than a nested frame. Other
recursive calls are SB5037 (see
[Functions, inlining, and recursion](#functions-inlining-and-recursion-dd27)).
There is no fixed nesting cap; allocation exhaustion follows the fatal ERR
protocol. Main, EXIT and ERR have distinct heads, so cleanup cannot overwrite
an interrupted capture's live frame.
The purity walk nevertheless treats a revisited function as impure, so a
self-tail function's capture currently takes the general path.

One consequence is deliberately observable: a pure body probing its own
stdout (`[[ -p /dev/stdout ]]`) sees the caller's real stdout, while
an impure one sees a regular file. Specs `cap.12` and `cap.17` pin both
halves. See journal 277.

### Terminal handlers and signals

EXIT is ordinary checked code; ERR requires explicit handling of operational
failures (DD128). Each is an optional generated function over real static
globals, with private locals/temporaries and a saved entry status. Neither
permits function definitions. The accepted termination contract selects by
cause: fallthrough, every explicit `exit [N]` and signals choose EXIT;
uncaught failures, explicit `fail [N]` and first fatal faults choose ERR.
Missing handlers do not substitute one another.

`exit` defaults to 0; `fail` defaults to 1. Both accept any i64 under the
integer-sink rules, including `fail 0`; only the final process status is
narrowed. Neither operation returns a catchable status. An `exit` inside
either handler terminates directly; `fail` can transfer EXIT to ERR, but
terminates directly inside ERR. This 2026-11-06 routing revision and the
new builtin are implemented in journal 576, following DG's journal 575
decision.

The runtime marks progress before calling a handler. EXIT may fail into
ERR, but neither re-enters and ERR never returns to EXIT. Children run
neither. Dispatch requires no allocation and preserves completed global
writes, including on heap startup failure; a second fatal fault terminates.
Kernel reclamation handles abandoned local resources, while shared reader
teardown and invoker-descriptor restoration precede the callback.

`terminal.rs` checks live same-process SSA operations for SB5101. It sees
actual lowered handling and inline call origins rather than guessing from an
outer `-` or `if`. Empty/NUL/regex/collection facts permit proven-safe values;
mutations invalidate those proofs. Arithmetic faults and heap exhaustion
remain fatal exceptions, not banned operations. DD130 adds fork failure
to that contract; its runtime routing and corresponding effect-model
updates remain backlog work.

FLAG and EXIT handlers record delivery in separate runtime atomics (DD#56).
Only EXIT requests shutdown; the most-recent-signal value alone cannot decide
it. With any EXIT declaration, both backends poll at external/pipeline and
loop/function boundaries and enter EXIT with `128+signal`. `signals.rs`
uses semantic source adjacency and the SSA CFG to defer dispatch across
acquisition/attest resolution, including calls and parent-reader completion.
It installs explicit checkpoints after discovery passes and marks backend
direct polls with the same protection. Receipt remains immediate, explicit
transfers retain precedence, and handler execution cannot dispatch again.
FLAG-only programs receive no polling instructions.

### Environment

`sash_getenv`, `sash_setenv`.

### File tests

`sash_file_exists`, `sash_file_is_dir`, `sash_file_readable`, etc.

### Arrays

`SashTable` — hash table implementation used for both indexed and
associative arrays (DD#33).

---

## Backends

### Native (primary)

Cranelift → static ELF binary. No libc, no dynamic linker. Result
is a ~16KB statically linked executable.

### C (the oracle)

`emit_c.rs` — `--emit-c` writes C on stdout instead of an object. The C is
compiled by `gcc` and linked against the same `libsash_rt.a` as the native
backend, so the two share a runtime and differ only in how the SSA reaches
machine code.

This is not a portability story; it is a **differential oracle**. The whole
spec suite runs twice, once per mode (`SASH_MODE=native` and `SASH_MODE=c`),
and a divergence between them localises a bug to one backend without a
reference implementation having to exist. It is the reason a codegen
regression usually announces itself as a single failing spec in one mode
rather than as a wrong answer in both.

### IR capture

`emit_ir.rs` — `--emit-ir` writes the IR as an **inert Janet data
literal**: a value the reader builds, not a program to execute.

This is the decoupling seam (DD#104). The in-tree Janet backend was
removed because it had stopped gating (41 permanently-tolerated
failures) while still taxing every new spec (~28% skipped). A future
oracle is a separate program that reads a capture, so it can lag the
compiler without blocking it.

Two properties are load-bearing: every match is exhaustive with no
`_` arm, so a new IR variant is a compile error rather than a silent
omission; and tags are IR node names, never an interpretation
(`attest` is `{:kind :command :name "attest"}`) — interpretation
belongs to the consumer.

### Manifest

`--manifest` prints the program's external-command manifest: every binary
the program can execve, resolved to an absolute path at compile time
(DD#23). It is what makes a compiled script auditable without running it.

Every path in it is one the compiler confirmed is a regular file with an
execute bit, in the resolution root (`--sysroot`, or the build host). Until
SB5095 that was not true: an absolute head was never looked at, so a
manifest could list a path the compiler already knew was absent — a
requirement that cannot be met, asserted as fact.

A resolution root that cannot be read is a **usage error reported before
compilation starts**, not a language diagnostic. An unreadable image used to
yield an empty listing, so every lookup in it answered "not there" and every
command head in the program reported SB5095 — one bad compiler argument
buried under a diagnostic per line, each blaming the user's source.

There is no globbing in command position. `*` is a glob only in an array
context (DD#39, scoped by DG on 2026-10-10), so `/bin/ech*` is a program
that does not exist rather than a pattern. A command name with no bare
spelling — a path containing a space — is written in single quotes, which
denote the literal name.

Every *quoted constant* head is judged at parse time, by whichever rule is
true of it (DG, 2026-10-11). Quoting a name that could have been written bare
is SB5096 ("write it bare"); so is a name that genuinely needs quoting but is
said some other way — double-quoted, or assembled from pieces — with the
opposite advice. One rule, so one code: a command name is one bare word, or
one single-quoted word when it has no bare spelling. An *empty* name is
SB4074, a syntax error rather than a resolution one: a zero-length name can
denote neither an intrinsic nor a program, and quoting says a name with no
bare spelling, not the absence of a name.

SB5011 is left for the heads that really are resolved at runtime, which is
what its text has always claimed. It used to receive six constant shapes as
well — `""`, `"echo"`, `"/bin/echo"`, `"ec"ho`, `ec"ho"`, `'ec'ho` — and tell
each of them that "a variable or expansion can't be resolved at compile
time", a cause none of them had.

### Source emission

`--extract <binary> [source|ir|manifest]` prints one of the note sections
that `codegen/entry.rs::embed_artifacts` writes into every native binary, so
a compiled program always carries the source it was built from, its IR
capture, and its manifest (DD#45, DD#109). The section defaults to `source`,
so the original `--extract <binary>` spelling is unchanged.

Because zero-length sections do not survive into the linked ELF, an empty
program loses both its source and manifest sections. The mandatory,
nonempty `.sash_ir` identifies that output; `.sash_source` also recognizes
older outputs. Absent source/manifest sections then read back as empty,
while an ordinary ELF, a missing IR, or a present unreadable/non-UTF-8
artifact still fails. The native codegen bundle requires all three texts.

Embedding the IR on every compile is what keeps the exhaustive emitter honest:
a variant that fails to render breaks every build instead of only the rare
`--emit-ir` invocation.

`--emit-source` and `--annotate` are **sash-convert** flags, not `sash`
flags (DD#57) — `sash` rejects them and says so. Source transformation is
the converter's job; the compiler only compiles.

---

## Tests

**The spec suite is the authoritative gate.** `cd spec && bash run.sh` runs
every `spec/*.sh` in both the native and C modes and accumulates results in
`spec.db` (SQLite, repo root, untracked — every run rewrites it wholesale,
and `runs` records a `commit_hash` per test so the database is its own
history).

Runtime fault/resource regressions require `strace` and util-linux `prlimit`.
`run_sash_under` applies their prefix only to the compiled binary, never to
the compiler or C linker. Syscall injection makes partial pipe/fork failures,
interrupted waits and signal checkpoint boundaries deterministic.

`sash/tests/` **no longer exists.** All 546 Rust integration tests, and the
`sash-rt-tests` crate, were deleted in the 2026-09-14 redux (journal 304)
after each was classified with per-test evidence: 210 asserted surfaces we
have deliberately retired, 272 were already covered by concrete spec ids, 45
were bash-differentials with no independent assertion, and only 4 expressed
anything a spec could not — those became 20 new spec tests.

The reason for going that far, rather than repairing: **the harness was
fail-open.** `run_bash_native` called `sash::codegen::compile_to_object`
directly, so `validate` never ran. 49 of its 159 passing tests (31%)
compiled programs the real compiler rejects — `echo -n`, for instance, is
rejected by SB5059 but still honoured by codegen. A third of that suite's
green was measuring a pipeline with a pass removed.

> If you build a new harness, **drive the real `sash` binary**. A harness
> that calls into the middle of the pipeline tests a compiler we do not ship.

`mk test` (Plan 9 mk, not make) covers C regex, `sash-grep`, `sash-getopt`
and in-crate Rust units (`sash`, `sash-convert`, `sash-expand`). Its Cargo
phases, and those of its `mk build` prerequisite, are chained with `&&`: a
failed compiler build/unit phase must not be hidden by later successful
crate phases.

---

## Key Design Decisions

Design decisions documented in `notes/decisions/`. The most
architecturally significant:

| # | Decision | Impact |
|---|----------|--------|
| 12 | Strings are (ptr, len) with null terminator | C interop without copying |
| 104 | Janet oracle decoupled via `--emit-ir` (supersedes DD#15) | Oracle is out-of-tree, post-beta |
| 24 | Taint quarantine | All external input tainted |
| 30 | Typed variables (int) | `declare int`, `fn -> int` |
| 33 | Unified hash table | SashTable for indexed + associative arrays |
| 34 | Minimize unsafe in sash-rt | Audit surface reduction |
| 39 | Star-only glob + regex mutation | `${var:s/re/str/flags}` |
| 43 | Attest semantics | Validation-gated declassification |
| 44 | Full taint quarantine | Read-blocked, not just command-blocked |
| 45 | Source emitter architecture | Four-axis declarations, const, export-as-error |
| 129 | Public rustix plus a Linux Rust syscall shim | Accepted runtime direction; implementation pending |

---

## Adversarial Review

One completed (audit/run-1/). 5 models, 3 passes, remediation done.
Validator bypass testing is the highest priority for future reviews —
finding Bash scripts that achieve dangerous effects while passing
the validator without diagnostics.

---

## For New Contributors

1. Read `notes/journal/001-prologue.md`. It sets the tone for everything
   else.
2. Read `notes/status.md` — the current priority and what was just
   finished. Read it first every session, not just the first one.
3. Read this guide.
4. Run the spec suite (`cd spec && bash run.sh`) to see where the tree
   stands. That is the authoritative gate; `mk test` currently does not
   pass at HEAD (see the Tests section).
5. Read `notes/decisions/` before changing any behaviour that might be
   deliberate. It is the authoritative record of what is intentional
   versus what is a gap — and the safe subset is defined by those
   decisions, not by any single prose document.
6. Pick a task from `notes/tasks/`. **Re-measure the filing against the
   binary before acting on it** — a filing is a snapshot of a moving
   tree, and a surprising number of them have aged out.
7. Follow the completion checklist in `AGENTS.md`.
