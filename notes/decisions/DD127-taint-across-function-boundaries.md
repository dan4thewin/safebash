title: DD#127 — taint across function boundaries
status: accepted

# DD#127 — taint across function boundaries

## The rule

**Own taint is attested in the body that produced it, or — when that body has
exactly one call site — immediately after that call. Received taint is never
attested inside a body.**

In full:

1. An attest lives in the body whose command produced the taint, as the
   immediately following statement, in the same block (DD#121 R1).
2. A call to a function with exactly one call site is that function's body
   transposed into the caller. The call is one compound statement of the
   caller, so the caller attests immediately after it.
3. A function with zero, two or more call sites is a firewall. Taint produced
   by its own commands is attested inside it, and none may leave it.
4. Taint a body receives — taint a name already carries when the body is
   entered — is never attested inside that body. The caller attests it before
   the call, or the body reads it with `:!` (DD#91).

DG, 2026-10-23: "I very much want taint/attest to be easy to reason about".
Everything below is a consequence of these four rules, or the mechanism that
checks them.

## Terms

- **Body** — a named function's body, or an anonymous `$( … )` body (DD#86,
  2026-09-29: "a body is a function body"). Main, EXIT and ERR follow rule 1
  like any body. Their entry is not an authored call site, so rules 2 and 3
  do not apply to the roots themselves. DD#128 defines each handler's
  incoming-data contract and requires fresh acquisition before its own attest.
- **Call site** — any syntactic call of a function: at the top level, in EXIT/ERR,
  in a body, inside a capture, as a pipeline stage, under `&`, or as an
  arithmetic call `(f …)` (DD#58). Sites are counted per function definition.
  - A call inside a loop is one site. The rule is about where an attest is
    written, not how often it runs.
  - A self-tail call (DD#27) is the function's loop, not a site.
  - A tail call to another function is a site of that function.
- **Single-use** — exactly one call site. An anonymous `$( )` body is
  single-use by construction.
- **Shared** — zero, two or more call sites. Zero counts as shared because
  there is no "after the call" to attest at.
- **Own taint** — taint produced by a body's own commands, including the
  commands of the single-use functions it calls and of the `$( )` bodies it
  contains, all of which are transposed into it.
- **Received taint** — taint a name already carries when a body is entered.

## Channels

Taint crosses a function boundary in three ways. The port's notes (journals
531-554, `notes/tasks/batch-531-findings.md`) call them T1, T2 and T3.

| channel | port name | what crosses |
|---|---|---|
| stdout | T1 | the function's output, read by `$(f)` or by `f \| reader` |
| names outward | T2 | a global or a collection slot (DD#100) the body leaves tainted |
| names inward | T3 | a global or a slot that is tainted when the body is entered |

These are not channels:

- **Positional arguments.** Passing a tainted value — `f "$x"`, `f @a` — is a
  read at the call, and SB5020 there.
- **The exit status.** It is compiler-managed, and always clean.
- **An int function's return value.** It is arithmetic; a tainted operand is
  SB5020 where it is read.

Stdout is judged where the output is consumed; names are judged where the
body leaves them. A function that prints to the terminal (`f` as a statement)
is normal: its output becomes data only when something captures or reads it.
A tainted global left behind is a hazard to every later reader, so it is
judged at the source.

## Rules by channel

| channel | single-use `f` | shared `f` |
|---|---|---|
| stdout | A kernel-backed `$(f)` or `f \| reader` is tainted; a compiler-owned buffer capture introduces no taint. Attest the tainted result where DD#121 puts the attest for an external command. | Capture and attest inside `f`, then export through a compiler-owned buffer. An outside attest of its kernel-read output is rejected, naming `f` and its call sites. The result stays tainted; `:!` at the use stays legal. |
| names outward | The call taints what `f` leaves own-tainted. Attest immediately after the call, or a later non-`:!` read is SB5020. | SB5067 at the source statement inside `f`, the last one per name. |
| names inward | A use that would be rejected if the received taint were `f`'s own is rejected with the same code, at the call. | the same |

Where DD#121 puts the attest for an external command — and so for a
single-use `$(f)` or `f | reader`:

- after `y=$(f)`: the next statement;
- `if y=$(f) {`: the first statement of each branch;
- `for x in $(f) {`: the first statement of the body;
- `f | while readline x {`: the first statement of the body;
- after `f | readline x`: the next statement.

**Names inward, in detail.** For a name a call passes in tainted, a statement
of `f` that uses it is rejected exactly as it would be if `f` had tainted the
name itself:

- a non-`:!` read is SB5020;
- a write into a collection cell is SB5021 (DD#61);
- an attest is SB5088, because it is never adjacent to the statement that
  produced the taint.

Each is reported at the call, naming the line in `f`. A body's verdicts
therefore do not depend on its callers: each definition has an independent
summary, reused at its calls. An anonymous `$( )` body is written where it is
called, so its errors point at the line in the body.

### The read boundary

**A kernel read introduces taint. A compiler-owned buffer copy introduces
none, and does not erase existing taint.** This applies to every fd read:
ordinary files, pipes and memfd captures, including pipes whose producer is
only `echo`. Environment and program-argument inputs remain tainted too.
An unchecked tainted value still cannot be read by `echo` or any other copy;
the ordinary use-site rules require attest or explicit `:!` first.

DG proposed this boundary on 2026-11-02 and chose to implement it using the
**existing buffer paths first**, without extending buffer eligibility in
this port. Consequently:

- `$(f)` can be clean when `f` captures input internally, attests it and
  emits with `echo`. The inner capture reads a kernel object; the outer
  capture returns a compiler-owned buffer.
- A direct external, any redirect, a pipeline, or a revisited function in
  the purity walk currently disqualifies a body from buffering. Its
  captured result is tainted even if every intended output byte was
  generated internally.
- `echo hello | readline x` taints `x`. A streaming wrapper using
  `cmd | while …` also has a kernel-backed outer capture. Attesting a value
  before writing it through a pipe does not attest the next kernel read.
- Integer/value calls return their values, not their stdout. Their stdout
  effects can still disqualify an enclosing capture from buffering.

Descriptor routing is not a trust proof. A pathname can alias `/dev/stdout`;
even an external redirected to `/dev/null` can reopen a parent's saved
capture descriptor through procfs. Both backends admitted such a program
before W2. No pathname exception or runtime alias guard is added.

Buffer eligibility is shared by the source analysis and lowering. The SSA
backstop classifies the actual buffer/kernel choice, not a separate
"trusted producer" flag. Further buffering optimizations may expand the
clean cases without weakening this rule. DD#88's SB4075 writer-preservation
check remains a separate syntax policy, not a taint-safety proof.

### Validation ownership

The read boundary determines **whether data is tainted**. The shared-function
rule separately determines **where it may be attested**. DG retained the
inside-function requirement on 2026-11-02: a shared external-producing
function captures and attests internally before exporting buffer-copyable
output. Merely moving a check before a subsequent kernel read does not
validate that read.

With the existing transport paths, even a shared builtin-only writer sent
through a real pipe can require `:!` at its receiver. A clean buffered
`$(f)` needs no attest; attesting that already-clean name remains harmless.
This is not a trusted-producer exception to the read rule.

DG also identified the optimization opportunity: a completely known
sash-generated pipe writer with no external calls could transfer through a
compiler-owned buffer instead. That must replace the kernel read, not mark
it trusted, and preserve DD#122's producer isolation, streaming, early-close
and status semantics. Wider buffering remains separate from this port.

### Chains

Transposition composes: a single-use function called from a single-use
function is transposed twice. An attest after a capture is legal only if every
function on every path from the capture to the producing command is
single-use. Otherwise the rejection names, for each offending path, the
innermost shared function — the one nearest the command, which must hold the
attest — and its call sites.

In example e7 below, `current` and `head_id` each have one call site, so the
attest after `$(current)` is legal. If `head_id` gained a second call site,
`head_id` would have to attest its output. If only `current` did, `current`
would have to capture and attest: `x=$(head_id); attest x …; echo "$x"`.

### Edge cases

- **A single-use call is a source site of its caller.** A statement that calls
  a single-use function, or holds a `$( )`, and leaves own taint behind taints
  those names exactly as an `argv` statement does: the attest follows it, and
  in a shared caller it is the site SB5067 reports.
- **Attesting a clean name** is a no-op, legal anywhere. The doc comment on
  `check_adjacency` (validate.rs), which implements DD#121's rule, says:
  "Attesting a name that is already clean stays harmless — that is a no-op,
  not a bypass."
- **An attest after a call, of a tainted name the call did not taint,** is
  SB5088: it belongs after the statement that did taint it.
- **A single-use call that re-taints a name** is that name's source. In
  `argv g; f; attest v in g …` with `fn f { argv g }`, the attest is legal. As
  DD#121 puts it, "a second `argv a` is still the operation that taints `a`,
  even though it changes nothing". The earlier taint, never read, is no error.
- **A shared function that re-taints a name** is SB5067, even when the name
  was already tainted on entry.
- **Changing the number of call sites.** Going from two sites to one never
  breaks a program. Going from one to two, or to zero, moves the obligation
  into `f`: SB5067, or the call-site rejection, with a message that names both
  sites.

## Why single-use and shared differ

Sash inlines every call, so a single-use call is literally the one place
where `f`'s commands run. DD#121 measures a compound statement as one
statement: "`h=$(getenv X) || h=DEF` taints `h`, so the attest below it is
adjacent." A single-use call is measured the same way — `f`'s body is one
compound statement of the caller — and the attest after it is adjacent to the
statement that produced the taint.

A shared body stands for several places at once. An attest at each call site
would give one command several attests, possibly with different predicates,
none of them beside the command. DG, 2026-10-23: "it only means the
line-by-line needs to be within the function".

The inward rule follows from the same picture. Transposed, `f`'s body is a
nested block of the caller, and R1 requires the attest in the same block as
the statement that tainted its subject. An attest inside a nested block, of
taint produced outside it, is not adjacent; that is already true of an `if`
block. This program (n1) is SB5088 today:

```
argv a
str c=y
if [[ $c == y ]] { attest v in a [[ -n $v ]] }
/bin/echo "${a[0]:-none}"
```

So DD#121 has three structural positions, not four. In each — a `while`
condition, an `if` condition, a `for` in-source — the tainting operation is
the syntactic head of the block the attest opens. A call does not produce the
taint it passes in, so a body is not a fourth position.

## Examples

Each program was compiled at 0149c49b; the verdict table preserves that
baseline. "W2b" marks the shared-output call-site rejection still to implement.

### stdout

e1 — a single-use capture, attested after:

```
fn branch { /usr/bin/git branch --show-current }
b=$(branch)
attest b [[ $b =~ ^[A-Za-z0-9._/\-]+$ ]]
/bin/echo "$b"
```

e2 — the same function captured twice, attested at each site:

```
fn branch { /usr/bin/git branch --show-current }
b=$(branch)
attest b [[ $b =~ ^[A-Za-z0-9._/]+$ ]]
c=$(branch)
attest c [[ $c =~ ^[A-Za-z0-9._/]+$ ]]
/bin/echo "$b $c"
```

e2r — e2 repaired; the shared function attests inside:

```
fn branch {
  cur=$(/usr/bin/git branch --show-current)
  attest cur [[ $cur =~ ^[A-Za-z0-9._/\-]+$ ]]
  echo "$cur"
}
b=$(branch)
c=$(branch)
/bin/echo "$b $c"
```

e2s — a streaming function attests beside its command, inside the loop
(legal whatever its number of call sites):

```
fn tracked {
  /usr/bin/git ls-files | while readline f {
    attest f [[ $f =~ ^[A-Za-z0-9._/\-]+$ ]]
    echo "$f"
  }
}
tracked | /usr/bin/head -2
```

e7 — a chain of single-use functions:

```
fn head_id { /usr/bin/git rev-parse HEAD }
fn current { head_id }
id=$(current)
attest id [[ $id =~ ^[0-9a-f]+$ ]]
/bin/echo "$id"
```

w1 — a pass-through wrapper, called as statements:

```
fn g { /usr/bin/git @args }
g status
g log
```

w1c — the wrapper captured twice, attested at each site:

```
fn g { /usr/bin/git @args }
s=$(g status)
attest s [[ -n $s ]]
l=$(g log)
attest l [[ -n $l ]]
/bin/echo "$s $l"
```

w1d — the wrapper captured once, never attested:

```
fn g { /usr/bin/git @args }
s=$(g status)
/bin/echo "$s"
```

A shared pass-through wrapper has three legal spellings: attest inside it
(one predicate for every use), write the command at each site, or write one
single-use function per purpose. "No exceptions" (DG, 2026-10-23) means there
is no wrapper exemption.

### names

e4 — a single-use function fills a slot; the caller attests after the call:

```
fn fill { argv A }
array a=()
fill =a
attest v in a [[ -n $v ]]
/bin/echo "${a[0]:-none}"
```

e5 — a shared function leaves a global tainted:

```
array g=()
fn load { argv g }
load
load
/bin/echo "${g[0]:-none}"
```

e5r — e5 repaired; the function attests what it filled:

```
array g=()
fn load {
  argv g
  attest v in g [[ -n $v ]]
}
load
load
/bin/echo "${g[0]:-none}"
```

slot4 — B115's reproducer; a single-use function fills a slot and attests it:

```
fn h {
  argv A
  attest e in A [[ $e =~ ^[a-z]+$ ]]
}
array g=()
h =g
for v in @g { echo "[$v]" }
```

e6a — a body reads a slot it received tainted:

```
fn show { echo "${A[0]}" }
argv a
show =a
```

e6b — a body attests a slot it received tainted:

```
fn check { attest v in A [[ -n $v ]]; echo ok }
argv a
check =a
```

e6r — e6a and e6b repaired; the caller attests before the call:

```
fn show { echo "${A[0]:-none}" }
argv a
attest v in a [[ -n $v ]]
show =a
```

e8 — an anonymous body taints an outer name; the attest follows the
statement that holds the body:

```
str t=clean
/bin/echo "$t"
str q=$(t=$(getenv HOME); echo x)
attest t [[ -n $t ]]
/bin/echo "$q $t"
```

c1 — a body writes its own taint into a cell:

```
fn f { A[0]=$(getenv HOME) }
array a=(x)
f =a
/bin/echo "${a[0]}"
```

c2 — a body writes received taint into a cell:

```
str t=$(getenv HOME)
fn f { A[0]=$t }
array a=(x)
f =a
/bin/echo "${a[0]}"
```

### Verdicts

The baseline column records 0149c49b, before W1; the implementation status
below distinguishes what has landed from what remains.

| example | before W1 | under this DD |
|---|---|---|
| e1 | compiles | legal |
| e2 | compiles | rejected (W2b): `branch` has two call sites |
| e2r | compiles | legal |
| e2s | compiles | legal |
| e7 | compiles | legal |
| w1 | compiles | legal: output to the terminal crosses no boundary |
| w1c | compiles | rejected (W2b): `g` has two call sites |
| w1d | compiles; prints git's output unchecked | SB5020 at the read of `$s` |
| e4 | compiles | legal |
| e5 | SB5067 | SB5067 |
| e5r | compiles | legal |
| slot4 | SB5020 at 7:10, a false rejection (B115) | legal |
| e6a | SB5020 at the read | SB5020, reported at `show =a` |
| e6b | compiles | SB5088, reported at `check =a` |
| e6r | compiles | legal |
| e8 | SB5067 | legal |
| c1 | SB5021 at the write | SB5021 at the write |
| c2 | SB5021 at the write | SB5021, reported at `f =a` |

## How it is checked

W1b-1 and W1b-2 implement **globals and collection slots** on the existing
per-statement transfer functions (journals 557 and 559). They do not
introduce an I/C/T state machine or re-walk a callee in each caller's
context. W2a implements read-boundary classification (journal 560).
W2b adds shared-output ownership to those same summaries (journal 561);
neither analysis re-walks a callee for a particular caller.

DD#128's strict resolution uses these same transfers and summaries
(journal 568). Ordinary clean writes preserve unresolved taint and its
owners. Only a legal adjacent attest or a clean whole-subject replacement
directly on its failed path resolves it. That replacement permission is
subject-specific and is reset at named-body, capture and process boundaries.
A genuine fresh acquisition replaces the old provenance and gets its own
adjacency; a literal overwrite cannot manufacture one.

### Building a name summary

For each definition of `f`, the compiler walks its body from an empty
tainted set, then once for each interface name, with only that name tainted
on entry. The interface contains every global and the five symbolic slots
A-E, including unused slots. The fixed slot domain adds five probes per
definition and avoids a second slot-arity analysis. These are symbolic input
probes, not walks for particular callers.

The collection-slot analysis relies on DD#100's **strict non-aliasing
contract**, settled by DG on 2026-11-02. Collection arguments have distinct
identities and cannot overlap globals accessed by the body or its callees.
The rule includes forwarding and anonymous bodies, applies statically and
has no read-only or taint-inert exceptions. It is a language check, not an
optional taint check.

This keeps one summary per function rather than introducing summaries
specialised by alias pattern, caller taint, or ordered alias-dependent
effects. It makes the independence assumption explicit for a reviewer.
It does not by itself establish taint safety: the transfers and diagnostics
below, including their control-flow handling, must still be sound.

The summary holds:

- **Generators:** interface names tainted at an exit of the empty-entry walk.
  These are `f`'s own taint.
- **Flows, indexed by incoming name:** interface names tainted at an exit of the
  singleton-entry walk. Independently generated names are excluded, except
  the incoming name itself: re-tainting an inherited name must not erase its
  incoming flow.
- **Conditional diagnostics, indexed by incoming name:** diagnostics
  added by that singleton input. The empty-entry diagnostics are subtracted
  as a multiset, preserving repeated sites rather than deduplicating by code.
  This records reads, cell writes and inherited attests with their original
  codes and locations. A read before a legal resolution remains an obligation
  even when its outgoing flow is empty.

An exit combines explicit returns with the final state only when the body
can fall through. Self-tail returns deposit a back-edge state instead; the
entry grows to a fixpoint before the summary is accepted. Ordinary loops
also separate break exits from continue/back-edge states. While exits retain
the final condition's state; for words are checked at the stabilized head,
and rebinding affects only an entered iteration, not the zero-trip path.
These joins carry ownership as well as boolean taint. Speculative passes
keep the enclosing loop depth but cannot write into the real body's exits
or obligations (DD#121, journal 562).

The census includes command calls and arithmetic calls, including bare
int-function names, captures and both handlers. Self-tail calls are excluded. DD#27
and SB5071 give a call graph with no legal inter-function cycles, so name
summaries are built bottom-up, without a cross-function fixpoint. A
multiply-defined name is conservatively shared and its definitions' summaries
are unioned; this does not model which definition is in force at each call.

### Applying a summary

First map the summary's names to the caller's identities. A-E map
positionally to the actual collection arguments: in `inner =B =A`, inner A
maps to outer B and inner B to outer A. Qualified function-local collections
use the same mapping. Unpassed slots have no mapping; a call without
collection arguments does not inherit the caller's slots.

A global normally maps to itself, except when that collection is passed
through a slot. SB5099 forbids the callee and its call chain from accessing
that global by name, so its dummy identity flow is omitted. Mapping it as
well as the slot would restore taint the slot just cleared.

At an ordinary statement call of `f`:

- For each mapped identity that arrives tainted, relay the formal input's
  conditional diagnostics at the call. The message names `f`, the actual
  caller identity and the original body location; the body's own text keeps
  its formal slot name. Transitive calls retain that chain. An inherited
  attest is therefore SB5088 at the call, never an entry adjacency exception.
- Replace the mapped names' state with their mapped outgoing flows. A name
  legally resolved on every continuing path disappears; an ordinary clean
  overwrite does not resolve received taint. A helper may acquire and resolve
  fresh input, but cannot inherit its caller's failed-attest replacement
  permission. Unmapped caller names are unaffected.
- Add the generators only for a single-use `f`. For a shared `f`, report
  SB5067 inside the definition instead, once per escaping global or slot,
  at its last recorded source. The message distinguishes a global from a
  collection slot, says why the function is shared and lists its call sites
  when multiple calls, rather than multiple definitions, make it shared.

Each definition's own diagnostics are reported independently of its callers.
Every body entry starts with an empty adjacency set, including singleton
probes of a received slot. `SlotInfo` and its inward/outward fixpoints are
gone; `slot_usage_map` remains solely for arity and dead-variable checks.
The top level starts empty. EXIT and ERR are independent roots, not walks
from main's final state. EXIT's incoming clean-use requirements are checked
at actual orderly entries, including explicit exits of any status and signal
polls. ERR conservatively receives any global that may acquire taint in main or
EXIT, even if a normal path later attests it. Requirements include helper
and slot uses and incoming-origin attest obligations. A partial fresh
acquisition cannot excuse an inherited-input attest on the other path.
DD#128's source analysis and SSA backstop enforce these contracts separately.

### Captures, value calls and child processes

An anonymous capture gets its own return frame, inherits the enclosing taint,
and merges its resulting taint back. It is single-use, so it has no SB5067
firewall. Its reads and inherited attests are checked inside the body.

Captures and arithmetic/value calls **union** their effects with the
incoming state, rather than clearing names they resolve. A capture or call
can be skipped by a lazy expression, or follow an earlier operand's read;
applying its resolution before checking that read would hide the error.
This remains conservative for genuine fresh acquisition/resolution in a
value. An ordinary clean overwrite, even when guaranteed eager, is not a
resolution at all under DD#128. Ordered-expression precision questions must
respect that distinction.

A background child and each forked pipeline stage are walked on a clone:
their reads still count, but their writes, returns and source log do not
change the parent. The final parent reader keeps its effects (the supported
`while` consumer and final `read`/`readline` paths). These calls still count
in the census. This is B18's validator half, landed with W1b-1 because
call-site effects make the separation necessary. W4's per-process
SSA backstop completes it (journal 563).

### Classifying output

`output_taint::OutputTaint` distinguishes value-returning calls from stdout
captures. Environment and argument values are tainted; arithmetic/intrinsic
results retain their ordinary operand checks. A real capture is tainted
unless `capture_purity::capture_body_is_pure` admits the buffer path.

That is the existing lowering predicate, moved out of `transform` for shared
use, not a new definition of purity. Its eligibility rules are unchanged.
Until DD#86's duplicate-definition rejection lands, the source classifier
includes every definition of a function name; one integer-return definition
cannot hide another definition's stdout. Lowering uses the body active at
the call, so source classification can conservatively reject a redefined
name whose particular call would buffer. This is a temporary implementation
boundary, not a supported redefinition contract. The task
`reject-duplicate-function-definitions` owns refusal of those programs;
do not add per-definition taint specialization to support them.

Every fd reader taints its target directly; the preceding pipeline stages
are no longer used to infer trust. `read SRC ARR` remains a value split.
SSA `ReadLine` and `ReadArray` are unconditional taint sources.
`CaptureEnd` carries only the emitted `buffered` choice, from which the
backstop derives its result. The former `clean_pipe` and capture `tainted`
annotations are removed. No output-analysis object is threaded into SSA
construction.

### Tracking output ownership

The tainted-name state also carries possible shared output owners. Joins
retain every possible owner. A genuine new acquisition replaces the old
provenance; an ordinary clean overwrite, split, unset or `+=` preserves
unresolved taint and its owners. A permitted failed-attest replacement clears
both together (DD#128). An attest of a clean name remains
a no-op. An attest of an owned tainted value is **SB5100**, naming the shared
producer and its call sites. Trust at a use (`:!`) remains legal: ownership
does not reject merely capturing or retaining the value.

Each existing function summary additionally holds:

- **Stdout paths:** direct output paths and paths already owned by a shared
  callee. A shared function takes ownership only of the still-direct paths,
  so a shared wrapper cannot hide an inner shared producer. Structural bodies
  and in-place integer calls contribute; an ordinary nested capture has its
  own output boundary. Pipelines and redirects remain kernel-backed. These
  paths never make a read clean or prove isolation from a pathname.
- **True origin flows and generators:** singleton probes carry an explicit
  incoming marker. Only outputs that retain it forward the caller's origins.
  These flows are separate from W1's boolean flows: W1 preserves an incoming
  name's identity when a fresh source re-taints that name, but the old output
  provenance must not survive that new acquisition. Clean value splits and
  whole-collection unsets now resolve both names and origins only when
  permitted by the directly enclosing failed attest; they no longer discard
  owners while leaving unresolved boolean taint behind.
- **Attest obligations:** an attest of a symbolic incoming name or stdin is
  recorded, then checked against the actual origins at a call. This covers
  named readers, helper chains and slot generators without caller-specific
  analysis. An external command may consume inherited stdin; it is not
  whitelisted by executable name.

The actual buffer/read classifier is consulted before assigning capture
ownership. A buffered `$(f)` therefore acquires no tainted owner, including
the internal capture/attest/echo repair. A later real pipe read does acquire
taint and the producer's ownership; internal validation is not a trusted-pipe
exception.

Stdin obligations follow DD#122's lexical boundary. A parent reader consumes
its private pipe; a function or anonymous capture called from that reader's
body uses ordinary stdin instead. Forked stages receive the preceding
stage's output as stdin. A redirected statement suspends the private reader
binding; an explicit stdin open introduces a new read source, still tainted,
while descriptor dups retain inherited-input obligations. This is source
ownership, not a physical descriptor-alias or isolation proof.

## Cost

The obligation depends on the number of call sites, a property of the whole
program. Adding a second call to a single-use function moves its obligations
into it: an attest after its capture becomes a rejection, and a name it
leaves tainted becomes SB5067. Both messages name the call sites, so the
cause is on the screen.

## Implementation status

The global/slot summaries, call-site relays, single-use name effects, shared
global/slot firewall and validator child isolation above are implemented.
W1a already fixed local capture ordering and duplicate reads (journal 555).
DD#100's strict non-aliasing contract is also implemented, as a prerequisite
to W1b-2 (journal 558): SB5099 rejects duplicate slot identities and overlap
with globals named anywhere in the callee's call chain, even with
`--no-taint`. W1b-2 retires the interim slot-taint analysis (journal 559).
W2a replaces producer-based capture/pipe trust with the read boundary,
sharing the existing buffer predicate with lowering (journal 560).
W2b implements shared-output ownership, SB5100 and symbolic stdin/name
attest obligations on the same independent summaries (journal 561).
W3 implements ordinary loop transfers without replacing those summaries
(journal 562).
W4 implements the per-process SSA backstop (journal 563): classify and check
one process's executable blocks, then recursively check its children with
independent copies of the completed parent view. Same-process closures
exclude descendants and keep the existing ascending block order. Main and
the two terminal handlers are separate executable roots; handler inputs
follow DD#128 rather than inheriting main's completed view.
Unbound function templates are not executable parent definitions; their
lowering also restores current variable versions without rewinding version
counters. Both corrections are needed to keep definition-time effects out
of real reads. Actual inlined calls and child reads remain checked.
Journal 568 adds full definition-classification convergence before uses.
Ordinary writes depend on the actual active predecessor definition, not
the version counter minus one. Failed-attest replacement can drop that
dependency only for its subject, and the new value is still classified.
Phi sources outside the executable process view do not contribute.

Collection cell reads, splats, table iteration and map-fill operands now
carry their reaching `VarRef`. A later fill cannot taint an earlier clean
read, and a later attest cannot clean an earlier read or derived value.
The backend still addresses the binding's same physical table storage.
Journal 569 completes W5 under DD#128: checked EXIT, explicitly handled ERR,
actual process-global values and generated terminal callbacks. A shared
signal plan protects semantic acquisition/attest pairs, including explicit
checkpoints and both backends' direct polls. Before-acquisition and
post-reader checkpoints are real runtime operations; ordinary failure or
explicit exit still takes precedence over deferred signal dispatch.

The following remain:

- **Precision follow-ups:** ordered value-call/capture effects and
  definition-specific summaries are recorded in
  `notes/tasks/taint-global-summary-followups.md`.
- **Unlowered redirect operands:** these retain the bare-name lookup in
  the completed process view. Children also still inherit the completed
  parent view, not a precise fork-time snapshot. The ordinary collection
  timing repair does not claim to close those separate boundaries.
- **Counted jumps escaping an anonymous body:** the lowering defect remains
  `bug-counted-jump-crosses-capture-boundary`. B94's definition-order hole is
  fixed; B106's requested ordinary clean-split declassification is instead
  superseded by DD#128's deliberate strict rule.
- **Arithmetic call arity:** a sexpr call with missing collection arguments
  can still escape SB5066 and reach a native ICE or invalid C output
  (`bug-sexpr-call-slot-arity-unchecked`). The mapper does not invent
  implicit slot arguments to accommodate that existing bug.

## Spec consequences

### Landed in W1b-1

- `bfb.18`, `bfb.20`, `bfb.21` now require SB5020 at the later read and no
  SB5067: each function has one call site.
- `bfb.23`, `bfb.24`, `bfb.28` are accepted. This replaces DD#121's
  2026-09-30 anonymous-body firewall with the single-use rule.
- `cov.266` now requires SB5020 at the read. `cov.264`, `cov.265`,
  `cov.267`-`cov.269` and `cov.341` each gained a second call site and retain
  their SB5067 assertions. `cov.270` is unchanged.
- `gflow.1`-`gflow.22` pin call-site timing, inherited diagnostics,
  generators, shared-source attribution, capture effects, process isolation,
  returns, self-tail fixpoints, the call census and conservative value merges.
- The other `bfb` cases, `atslot.7`/`.8` and the collection-parameter specs
  retain their verdicts in this globals-only unit.

### Landed before W1b-2

- `100.9` now requires SB5099 for duplicate slot identities; the source is
  unchanged and its former alias permission is deliberately retired.
- `noalias.1`-`.16` pin direct and forwarded duplicates, direct/transitive
  global access, metadata, writes, attests, captures, arithmetic calls,
  conditional paths and cleanup bodies. Unrelated globals and literal `=name` words
  remain legal; `--no-taint` does not bypass the contract.

### Landed in W1b-2

- `100.27` remains SB5020 but moves to the call; `100.28` becomes
  SB5088 there. `100.29`/`.31`/`.33` retain their verdicts; `100.30`/`.32`
  remain SB5020 at the read.
- `sflow.1`-`.18` pin mapped cleanup, forwarding/permutation, conditional
  reads and attests, shared-slot sources, preservation of received taint,
  captures, background reads, the fifth slot and local/mixed collections.
  `.5` originally pinned the intermediate SSA timing rejection. Journal 568
  replaces it with `sflow.19`, requiring the same source to compile and print
  `clean` then `dirty`; the normal collection timing defect is fixed.
- `atslot.9` pins an own-fill caught attest whose failing cell exits. Along
  with existing `atslot.7`/`.8` and the `sflow` cases, this replaces the
  useful claims of the branch's `atslot.1`-`.6` without restoring their
  now-rejected inherited-attest premise or duplicating existing specs.
- B115 is fixed at statement calls, including an already-tainted actual
  collection. c2 was rechecked and already had its call-site SB5021 under
  W1b-1; it is not a new fix in this unit.

### Landed in W2a

- `outflow.1`-`.14` pin kernel-backed function/body captures, builtin and
  function pipes, nested capture/attest/echo buffering, and the separation
  between integer return values and stdout effects.
- `sw.26`-`.28` exercise the actual SSA reader/capture classifications with
  the front-end taint pass disabled. Existing `bkstp.6` remains a buffered
  capture's clean control.
- Sixty-three existing fixtures needed adaptation. Their runtime, grammar,
  status and lowering assertions are preserved with an adjacent attest,
  localized `:!`, or the existing `--no-taint` test switch. `dd82.4`, whose
  assertion was specifically clean user-function capture, now requires
  located SB5020. The fifteen formerly parked cases are among these, not
  left failing until W2b.
- The old B35 clean-function-pipe expectation and B114/e5 clean streaming
  or redirected capture expectations are deliberately superseded. A nested
  capture/attest/echo wrapper already buffers and remains clean
  (`outflow.11`); widening eligibility is separate work.

### Landed in W2b

- `outown.1`-`.34` pin shared and nested producers, multiple output paths,
  scalar/array/loop readers, caught and converting attests, global/slot
  summary mappings, returns, in-place value calls and symbolic stdin.
- Replacement and lexical-scope controls distinguish true origin flow from
  boolean taint, nested captures from in-place calls, and a private reader
  pipe from the ordinary stdin of its calls and captures.
- Existing buffered-capture controls (`outflow.10`-`.13`) remain valid.
  The new ownership rule does not widen buffering or alter the SSA backstop.
- `pipeown.4` uses explicit trust for its shared builtin writer's real pipes.
  Its triple-nested reader scope and exact eight-line output assertion are
  unchanged.

### Landed in W3

- `loopflow.1`-`.21` cover break/continue exits, counted and oversized
  targets, zero-trip binding, while-condition effects, live collection
  heads, per-iteration rebinding and output ownership.
- Single-use summaries retain loop-generated globals; shared functions
  report SB5067 at the original source. Quiet inner-loop probes preserve
  target depth without exporting their effects.
- Seventeen cases fail before the change: fifteen missing refusals and
  two false rejections. Four preservation cases pass before; the ordinary
  nested-capture control does not claim to fix the counted-capture bug.

### Landed in W4

- `procflow.1`-`.6` accept clean parent and sibling reads after private
  child writes, including background jobs, nested jobs and function stages.
  Both taint layers remain enabled for these positives.
- `.7`-`.14` isolate the SSA backstop. Child cleanup and attests cannot
  clean the parent; sibling cleanup cannot clean another child. Inherited
  child taint, grandchild reads and actual parent-side readers remain
  checked. Lifted command-argument stages are child processes too.
- `.14` rejects definition-only scalar cleanup: a function that is never
  called cannot change the SSA version read by its parent.
- Eleven checks fail before the change in each mode: six false rejections
  and five missing refusals. `.9`-`.11` are preservation controls.
- `bindscope.40` replaces `.39`, whose diagnostic assertion depended on an
  unreachable template. Its collision now uses an actual call's version,
  preserving the structured-name restoration claim rather than merely
  loosening the expected suffix. Journal 568's fresh initial definitions move
  the live versions again: `.41`/`.42` replace `.38`/`.40`, with `.42` using
  a real `t_4` collision. The joined-token restoration reversion still fails
  exactly those two checks in both backends.

### Landed as DD#128's strict-resolution prerequisite

- `adjresolve.1`-`.18` cover ordinary writes, self-copy, arrays, loops,
  buffers, helper and slot barriers, R2 without a later read, subject-specific
  permission, direct fallback and fresh helper acquisition/resolution.
- `adjpolicy.1`-`.9` replace old assignment-kill expectations without
  erasing unrelated claims. `.1`-`.6` replace `div.8`, `gflow.18`,
  `outown.17`, `.33`, `.34` and `loopflow.10` with strict refusals.
  `.7`-`.9` adapt `loopflow.14`, `.15` and `.20` using legal resolution.
  `.9` retains a tainted counted-continue deposit and pins executable phi
  inputs; it does not attest immediately before that continue.
- `adjssa.1`-`.16` independently cover live-binding continuity, named and
  anonymous barriers, full classification convergence, four collection
  timing channels and inherited private taint.
- `flife.1`-`.8` cover fresh named/captured/child/tail entries, real scalar
  and collection defaults, and preservation of enclosing locals and globals.

### Landed as DD#128's terminal-handler implementation

- `handler128.1`-`.17` pin the new surface, uniform SB5071, full-i64 routing,
  status overrides, independent source entry requirements and partial-refresh
  refusals for scalars and mapped slots.
- `errentry.1`-`.8` independently pin inherited-input restrictions through
  the SSA backstop. `errfail.1`-`.21` and `errproof.1`-`.8` pin actual lowered
  operational failures and the conservative value proofs that permit safe
  forms.
- `sigguard.1`-`.15` cover protected adjacency, function/loop/reader polls,
  false-reader entry, pre-acquisition dispatch and transfer precedence.
- `errresource.1`-`.12` cover capture setup failures, heap and arithmetic
  faults, once-only entry, separate live capture stacks and child isolation.
- Existing END fixtures migrate to fresh `h128.*` identities, retaining
  their relevant assertions under the selected handler's new contract.
  The branch's `endck` surface is not ported as a compatibility alias.

## Open

- Buffer eligibility beyond the existing paths
  (`q-buffer-capture-eligibility`); kernel reads remain tainted.

## Provenance

- 2026-10-23 (journal 531, lines 79-94). DG recalled that a user function
  could break the "always clean" verdict on its capture "by simply running a
  command/pipeline at the end", and ruled:

  > If a user function is used in a capture and the variable is used without
  > the :! escape hatch, I think that should also be an error, as the attest
  > belong immediately after, but a function without its own attest means the
  > same command must have multiple attests...

  Then: "it only means the line-by-line needs to be within the function";
  "yes to (c), no to exceptions."; "I very much want taint/attest to be easy
  to reason about. I think that means (c) for globals and out-params."; and
  "A; yes to Q3" — A being the strict reading for names going outward, Q3
  the inward rule.
- 2026-10-31. DG:

  > A function whose only call site is a capture would be equal to the
  > function's body transposed into the $( ), in which case the attest could
  > immediately follow the $( ) and satisfy the adjacency rule.  The would
  > allow the ergonomic case of encapsulating a command/pipeline as the sole
  > body of a function.

  The AI proposed (b): apply the same transposition to names going outward,
  which narrows the 2026-10-23 firewall to shared functions. DG: "go with b -
  do a full writeup focused on clarity".
- 2026-11-02. After a probe showed an external reopening its parent's saved
  memfd capture even with stdout redirected to `/dev/null`, DG proposed:

  > in the prior f example, capturing the output of the internal echo can
  > be a simple buffer copy rather than a pipe; we could apply taint on any
  > read(2) call, while leaving buffer copies clean

  DG selected **existing buffer paths first**, rather than extending their
  eligibility during W2a. This replaces the earlier reaches-stdout/pathname
  approach, not the independent name summaries.
- Decided by the AI and reported (DG may veto): a new DD rather than an
  addendum to DD#100; inward errors reported at the call, with the code the
  statement would raise; attesting a clean received name stays legal; zero
  call sites count as shared; the counting rules under Terms; the chain
  rejection names the innermost shared function.

## What this replaces

Each text below is rewritten in place to agree with this DD; the earlier text
is in git at 0149c49b.

- DD#24: "User-defined sash functions are trusted: `$(user_func ...)` capture
  is clean", and "`user_func | read x` → clean".
- DD#63: "a user-function capture is clean".
- DD#100 Part E: the slot attest "deferred into `f`", inward slot taint as a
  union over call sites, and a callee's diagnostics that depend on which call
  instantiated it. Outward slot taint is kept for single-use functions.
- DD#121: the fourth structural position ("DD#100 slot arrival → the body"),
  and the 2026-09-30 addendum (SB5067 as a net-effect rule, extended to
  `$( )` bodies). Its successor is the section "SB5067 — what a body may
  leave behind".
- AGENTS.md's Taint system list, which counted "user function capture" as
  clean.
- The 2026-10-23 reading of T2, under which every body was a firewall for
  names going outward. It survives for shared functions.

## See also

DD#24 taint model · DD#61 cell writes · DD#86 bodies · DD#91 `:!` · DD#100
collection parameters · DD#121 adjacency
