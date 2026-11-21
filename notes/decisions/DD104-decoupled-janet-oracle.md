title: DD#104 — the janet oracle is decoupled, and deferred to post-beta
status: accepted
depends: [DD15]
supersedes: DD15

1. `--emit-ir` emits inert, versioned Janet data; an independent oracle consumes
   that capture as a separate program without coupling compiler delivery to it.
2. Emit every IR node exhaustively, without wildcard fallbacks or semantic
   interpretation; tags name IR nodes.
3. Consumers must refuse unsupported schema versions. The capture retains a
   compound cell assignment's arithmetic operator, single index and unexpanded
   right-hand side. Provisional DD136 schema 4 also records each command's
   explicit `:best-effort` boolean, rather than erasing the attempt prefix.
4. Optional EXIT and ERR bodies have separate fields; absent differs from empty.

---

Settled with DG 2026-09-12.

The janet backend is **removed from the compiler**. The oracle idea is
not abandoned — it is *decoupled*. `--emit-ir` writes the IR as an inert
janet data literal; a future oracle is a **separate program** that reads
that capture. That effort is **post-beta**, and nothing before beta
waits on it.

DD#15 is superseded. Its core claim — two independent implementations
agreeing is epistemically stronger than one implementation matching a
reference — is not repudiated. What changed is *where the second
implementation lives*.

## Why

The oracle stopped paying for itself, and the numbers were unambiguous
once measured rather than assumed:

| mode | passing | failing | skipped |
|---|---|---|---|
| native | 2095 | 0 | 0 |
| c | 1920 | 0 | 27 |
| **janet** | **1366** | **41** | **540** |

Three facts together:

1. **It was not gating.** Those 41 failures are permanently tolerated —
   every sweep report for months reads "41 failing (all janet,
   unchanged)", and the pre-commit hook checks fmt, clippy and
   complexity, never janet. An oracle that cannot fail a commit is not
   an oracle; it is a report nobody reads.

2. **It taxed every new feature anyway.** 540 skips and **300
   `in_modes native,c` guard sites** across 28 spec files exist solely
   to route around janet. The skip ratio sat at **27.7–28.0% on every
   commit sampled** — a flat ~28% surcharge on new spec work, not an
   accelerating one, which is worse than it sounds: a constant rate
   never argues for itself and never goes away.

3. So the project was **paying the coupling cost without collecting the
   benefit.** That asymmetry, not the failure count, is the decision.

DG: *"every change seems to yield more janet bugs/gaps."*

## What the 41 failures actually were

Not noise, and worth recording because a restart should not rediscover
it. Thirteen are DD#56 signal specs; `cf.f9/f10/f12/i2` are the genuine
control-flow gaps. Together they confirm the 2026-08-16 diagnosis in
[the historical primitive-library task](https://github.com/dan4thewin/safebash/blob/6e99b1f1845a7159b0ba5dd33b745d13c1bf2403/notes/tasks/janet-rt-primitive-library.md):
the backend re-implemented
leaf primitives inline (spawning fake `getpid`/`read`/`regex` commands)
instead of calling a real primitive library. That task file also names
the exact trigger for this decision — "only if janet's purpose shifts
from oracle to portable execution target" — which is the fork this DD
takes the other side of.

## The capture

`--emit-ir` emits **data, not a program**. That distinction is the whole
mechanism, and it is easy to miss in a homoiconic language where the two
look alike.

- `--janet` emitted *semantics* — "what the program does", idiomatic and
  readable beside the sash. Correct semantics for 34 node types is a
  large, permanently-owed debt.
- `--janet-map` emitted *structure* via `sash/*` **macros** — "how the
  program parsed". Better, but a macro is a program: the consumer's
  preamble must match the compiler's vocabulary exactly, or it breaks.
  It also still had semantics to defer, and so still emitted
  `(sash/TODO stmt:arith-test)` — it could not render a 7-line program.
- `--emit-ir` emits the same structure as an **inert literal**. There is
  nothing to execute and nothing to version-match, so a consumer weeks
  behind the compiler can still read a capture.

Inertness is what buys the decoupling. It is also why the capture was
**complete on day one** — with no semantic decisions to make there is no
staging, no `TODO`, and no partial coverage.

Two properties are load-bearing, and a change that breaks either is a
bug, not a refactor:

- **Exhaustive, with no `_` arm anywhere.** Including the two halves of
  the `expr` split, which name each other's variants explicitly. Adding
  an IR variant is a compile error at every site that must learn about
  it — never a silent omission. A capture that quietly drops a node is
  worse than no capture.
- **Tags are IR node names, never an interpretation.** `attest` renders
  as `{:kind :command :name "attest"}`. There is no `:attest` tag and no
  run/ok? distinction, because that is position-based interpretation and
  belongs to the consumer. (Note also that `bless` is dead vocabulary —
  zero source hits; DD#15's amendment text predates the rename. It
  should not be resurrected.) Keeping this layer judgement-free is
  precisely what lets a consumer lag the compiler safely.

`:ir-version` is stamped so a consumer can refuse a capture it does not
understand, rather than mis-read it.

DD#128 introduced **`:ir-version 2`**: optional terminal bodies in
`:exit-block` and `:err-block`, replacing `:end-block`. An empty declared
handler remains distinct from an absent handler.

The indexed-update repair advanced the capture to **`:ir-version 3`**
(2026-11-13). An arithmetic cell update keeps its operation in the assignment's
`:op`: `:add`, `:sub`, `:mul`, `:div`, `:mod`, `:bit-and`, `:bit-or`,
`:bit-xor`, `:shl` or `:shr`. The assignment contains one `:index` and the
original `:value`, not a second index inside a synthesized RHS cell read.
Postfix `++`/`--` use `:add`/`:sub` with arithmetic literal 1. Ordinary
assignment and `+=` keep `:set` and `:append`; scalar arithmetic mutations
retain their existing expanded representation.

The **provisional DD136 trial** advances it to **`:ir-version 4`**. Each
command has a `:best-effort` boolean: true for `: function args...`, false
for ordinary commands, including bare colon. The command still names the
actual function and retains its ordinary arguments and redirects. A consumer
must not interpret a resultless attempt as a normal call or ignored data.

Schema 4 retains the earlier compound-update and handler fields. Both
`--emit-ir` and the embedded/extracted IR use the same schema. The data
remains inert; consumers must refuse versions they cannot interpret rather
than silently dropping a handler, attempt marker or compound update.

## Where the old code went

Tag **`janet-babel`** (annotated, pushed) is the last commit with the
backends intact. It is a quarry, not a branch — the tag message inventories
what is worth mining. In short: `emit_janet.rs` (~2400 lines) is the
closest thing to a specification of sash semantics in a second language,
and its 1366 passing specs are evidence most of it is right;
`emit_janet_map.rs` holds the `sash/*` vocabulary; the ~300 `in_modes`
guard sites document exactly which constructs janet never reached.

**Do not branch from `janet-babel` to continue the old design.** The
point of `--emit-ir` is that the next attempt consumes a capture instead
of living inside the compiler.

At DG's request, the 32 deferred Janet task records were removed on 2026-11-16.
[Journal 638](../journal/638-backlog-retirement-and-closeouts.md) records
that cleanup; the full records remain retrievable in the
[pre-cleanup task snapshot](https://github.com/dan4thewin/safebash/tree/6e99b1f1845a7159b0ba5dd33b745d13c1bf2403/notes/tasks).
This retires the old backlog, not the independent post-beta oracle idea
or the inert `--emit-ir` contract. Earlier journals' task names are
historical references to those retained Git records.

## Consequences at adoption (2026-09-12)

The task counts and C-task statuses below describe that adoption, not the
current backlog.

- Removed: `emit_janet.rs`, `emit_janet_map.rs`, `sash/janet/sash-rt.janet`,
  `sash/tests/janet_tests.rs`, the `--janet`/`--janet-map` flags, and the
  janet `SASH_MODE`.
- The spec suite carries **no janet dependency**. Specs 104.1–104.4 pin
  the `--emit-ir` surface and shape by string-matching the output; they
  deliberately do not shell out to janet.
- The emitter's parseability is verified **over the whole corpus, not
  per-commit**: 1974 emissions, every one parsed by janet.
- 24 janet-only task files move to `deferred`, pointing here. Six
  `c-janet-*` / `cjanet-*` tasks are **rescoped to c-only, renamed `c-*`
  and stay `ready`** — those are real c-backend gaps, visible as 27
  c-mode skips, and they do not go away with janet. (A seventh was
  already `done`.)
- `oracle_tests.rs` and `sash/src/oracle.rs` go too: despite the neutral
  names, both are janet — the file compiles to janet, runs it, and
  compares against bash. Nothing else used them.
- `spec/cli.sh`'s `cli.13` — "the usage text covers every advertised
  flag" — failed on the removal, which is exactly what it is for.
