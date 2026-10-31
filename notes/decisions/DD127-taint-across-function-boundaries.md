---
title: DD#127 — taint across function boundaries
status: accepted
---

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
  2026-09-29: "a body is a function body"). The top level and END follow rule
  1 like any body. They have no call sites, so rules 2 and 3 never apply to
  them; what END receives on entry is W5's question.
- **Call site** — any syntactic call of a function: at the top level, in END,
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
| stdout | `$(f)` or `f \| reader` is tainted iff unattested external output can reach `f`'s stdout. Attest it where DD#121 puts the attest for an external command. | The attest must be inside `f`. An attest of a tainted capture at a call site is rejected, naming `f` and its call sites. The capture stays tainted; `:!` at the use stays legal. |
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
therefore do not depend on its callers, and each body is checked once. An
anonymous `$( )` body is written where it is called, so its errors point at
the line in the body.

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

Each program was compiled at 0149c49b; "today" is that compiler's verdict.
"W2" marks the call-site rejection whose code W2 picks.

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

| example | today | under this DD |
|---|---|---|
| e1 | compiles | legal |
| e2 | compiles | rejected (W2): `branch` has two call sites |
| e2r | compiles | legal |
| e2s | compiles | legal |
| e7 | compiles | legal |
| w1 | compiles | legal: output to the terminal crosses no boundary |
| w1c | compiles | rejected (W2): `g` has two call sites |
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

This is W1's plan for the name channels; W2 adds stdout. W1 rewrites this
section in place if the mechanism it lands differs.

- Each body is walked once, from an entry where every name is **I**
  (inherited, unknown). A name's state is a set drawn from {I, C (clean),
  T (own-tainted)}.
- The walk produces a summary of `f`:
  - **R_f** — uses of possibly-I names that would be rejected if the name
    were tainted, each with its code: SB5020 for a read, SB5021 for a cell
    write;
  - **A_f** — attests of possibly-I names;
  - **K_f** — names with no I at exit (overwritten on every path);
  - **G_f** — names possibly T at exit;
  - **W_f** — the stdout witness: a path by which unattested external output
    reaches `f`'s stdout (W2).
- At a call of `f`, with S the set of names tainted at the call:
  - each name in S ∩ R_f is reported at the call with its recorded code,
    naming the line in `f`;
  - each name in S ∩ A_f is SB5088 at the call;
  - for a single-use `f`, the state after the call is (S − K_f) ∪ G_f;
  - for a shared `f`, a non-empty G_f is SB5067 inside `f`, reported once,
    and the state after the call is S − K_f.
- When the caller is itself a function `g`, a possibly-I name at the call
  passes into R_g or A_g, for `g`'s own callers to decide; a possibly-T name
  is reported at the call, in `g`.
- DD#27 (no recursion except a self-tail call, which lowers to a loop) and
  SB5071 (a function is defined only at the top level or in END) make the
  call graph a DAG plus self-loops. Summaries are computed bottom-up, with an
  intra-procedural fixpoint for a self-tail loop and no cross-function
  fixpoint.
- The top level starts with no I. END's entry state is W5's.
- A call as a pipeline stage or under `&` is a site. Its R_f and A_f apply;
  its K_f and G_f stay in the child, whose names die with it (W4).
- There is no context sensitivity: one call site is one context.

Today `taint_cmdsub_bodies` walks an anonymous body on a clone and discards
the result. That is sound only because SB5067, applied to `$( )` bodies,
forbids a body to leave taint behind. W1 applies the body's summary, keeping
G, instead.

## Cost

The obligation depends on the number of call sites, a property of the whole
program. Adding a second call to a single-use function moves its obligations
into it: an attest after its capture becomes a rejection, and a name it
leaves tainted becomes SB5067. Both messages name the call sites, so the
cause is on the screen.

## Implementation status at 0149c49b

None of this is implemented yet. W1 implements the name channels and W2 the
stdout channel. Until then the compiler applies the earlier rules, and the
specs pin them:

- every user-function capture and pipe is clean (until W2);
- bodies are taint-walked in two phases, as `docs/GUIDE.md` describes;
- SB5067 is a net-effect rule ("no net insert", measured against the body's
  entry set), applied to `$( )` bodies too; DD#121's History section
  records it;
- `analyze_slots` makes inward slot taint a union over call sites, and
  outward slot taint a fixpoint;
- an attest in a body, of a slot the body received, is accepted;
- B115: an attest in a body, of a slot the body filled, does not clear the
  caller's collection, so the caller's later read is a false SB5020 (slot4
  above, both modes).

## Expected spec consequences

W1 and W2 confirm each of these by measurement.

- Unchanged: `bfb.17` (rc 0), `bfb.19`, `bfb.22`, `bfb.25`, `bfb.26`
  (SB5088), `bfb.27`; `atslot.7`, `atslot.8`.
- `bfb.18`, `bfb.20`, `bfb.21`: SB5067 becomes SB5020 at the later read
  (`for v in @g { /bin/echo $v }`), because each function has one call site.
  `bfb.18` asserts `*SB5067*leaves\ the\ global*`.
- `bfb.23`, `bfb.24`, `bfb.28`: SB5067 becomes accepted. This reverses the
  2026-09-30 extension of SB5067 to `$( )` bodies: an anonymous body is
  single-use, so its caller attests after it.
- collparam `100.27`: still SB5020, now reported at the call. `100.28`: rc 0
  becomes SB5088 at the call; W1 adds a spec in which a body fills a slot and
  attests it. `100.29`, `100.31`, `100.33`: unchanged. `100.30`, `100.32`:
  SB5020 at the read, unchanged.
- coverage `cov.264`-`cov.269` and `cov.341` pin SB5067's two raise paths (a
  bulk fill and an assignment into a global), each with a function that has
  one call site, so none of them is SB5067 under this rule. `cov.266` reads
  the global afterwards and becomes SB5020 at the read; the others never read
  it again, and W1 measures what they raise. W1 gives each function a second
  call site so that both paths stay pinned. `cov.270` (rc 0, an attested
  local) is unchanged.
- A shared function that leaves its own taint behind becomes SB5067: one that
  re-taints a global (the glob1 shape), or fills a slot and does not attest
  it. W1 measures which specs have either shape.
- Fifteen specs pin today's clean user-function capture:
  `supval.2/3/6/7/8/14/16/18/19/21/22`, `cap.20`, `88.28`, `cmt.11`,
  `dd82.4`. They change at W2, which re-measures each under the single-use
  rule.

## Open

- W1: the span reported for a single-use call as a source site; `len A` on a
  tainted collection; whether SB5067 suppresses later errors for the same
  name; c2's code; SB5067's message, which says "taint may not escape the
  body that creates it" and must instead say that the function has zero, two
  or more call sites.
- W2: the code of the call-site rejection; the reaches-stdout analysis
  (B114 lists what the branch's version got wrong).
- W4: the name effects of a stage or `&` call.
- W5: END's entry state (B96).

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
