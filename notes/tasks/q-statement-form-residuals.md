---
title: "Enforce statement boundaries, uniform binding lists and local capture/attest pairs"
status: open
priority: P4
kind: work
depends: []
gates:
  - "DD71's whole-statement boundaries need no extra separator; a value's closer is not a statement boundary, in native and C"
  - "one or several plain assignments and typed declarators have consistent list boundaries and ordered effects; a following statement requires an explicit separator unless a recognized enclosing closer or EOF ends the list"
  - "DD121 adjacency is per binding, not per list: reject batch or late attests, including after an intervening clean binding, preserving individual pairs, clean captures and explicit :! reads"
  - "preserve existing type, scope, const, result-kind, operand-word, capture-condition and signal-resolution contracts without introducing environment-prefix or transactional semantics"
  - "coordinate retired-group and invalid-name diagnostics with their owners; their earlier refusals must not masquerade as proof of the non-group boundary rule"
  - "take a current fixture census, allocate fresh ids for changed contracts, and prove each new guard with real CLI before/reversion controls before the predicted gate"
  - "update the source/spec citations of this task to the accepted rule, retaining only coverage of legal forms or the proper refusal"
---

## Accepted policy (DG, 2026-11-09)

DG first rejected the implicit assignment-to-command split, then asked
for the declaration asymmetry and self-delimiting forms to be considered
together. After that discussion:

> yes to no extra separators when there are clear boundaries and yes to ;
> after >=1 assignment. a capture followed by an attest should be a unit
> rather than multiple catures in one statement followed by multiple attests

DD71 is authoritative for the grammar; DD121 for binding-level adjacency.
DD53, DD68, DD128 and GUIDE agree. Journal 599 records the discussion.
The questions are settled; implementation remains open in this task.

### Visible boundaries, not mandatory semicolons everywhere

Completed brace-bodied forms, `fi`/`done`, and otherwise legal
self-delimiting statement forms need no additional separator. Preserve
`if /bin/true { echo yes } echo done`, a function definition followed
by a call, and a mutating `(( ... ))` followed by a command.
An enclosing closer or EOF ends the last statement without a semicolon.
Recognize the grammatical role, not just the spelling: `else`/`elif`,
connectors and legal redirects remain attached to their own construct.
This does not legalize a pointless bare test or a retired command group.

Quotes, `${...}`, `$()`, `$((...))` and collection-literal closers are
inside operands. They do not end the surrounding binding list or
command. A command remains open-ended; do not split its arguments by
recognizing a later word as another command.

### Uniform binding lists

These are separate valid examples:

```sash
a=one b=two; echo "$a $b"
str a=one b=two; echo "$a $b"
int a=1 b=2; echo "$a $b"
```

Before a following statement, one binding needs the same `;` or newline
as several. Reject `a=one b=two /bin/echo hi`; a declaration cannot run
that following command either. Bare declarators remain legal, so
`str a b` is not inferred to mean a declaration followed by a call to
`b`. Do not use command/function lookup to guess declaration boundaries.

Evaluate and write each binding before the next, preserving source
order and existing per-binding constraints. No environment prefix,
scope, atomicity or simultaneous RHS snapshot is introduced. Lists
produce no result or status and gain no condition/pipeline/background
eligibility. Preserve the existing DD88/DD101 capture-condition forms,
not a new result-bearing binding-list extension.

### Local acquisition/resolution

Each binding, declared or plain, advances adjacency independently.
A list cannot acquire several subjects and then grant adjacency to
several later attests. A clean binding also intervenes; the final
binding alone can have the immediately following attest.

Keep `a=$(getenv A); attest a p?` together before acquiring and attesting
`b`. This is an ordinary acquisition/resolution pair, not new syntax.
It preserves DD128's signal-deferral unit and failed-path obligations.
Grouping must not silently change locality because one AST node happens
to contain several bindings.

Do not turn this into a ban on all grouped `$()` expressions or mandate
attests for clean string/integer producers. DD91's explicit `:!` remains:
an earlier unresolved binding can be read through it, but a later
binding's attest does not clear the earlier one. Whole-collection
attest-all and the existing structural adjacency positions also remain.

## Ownership and historical evidence

**Policy boundary, 2026-11-09:** general command groups are retired.
Both `cmd && { a; b }` and `cmd || { a; b }` remain solely as their
corresponding `if` shorthands (DD70/DD95, journal 596).
`retire-command-groups` owns enforcement, including `return { ... }`
and groups accidentally split off after another statement.
Q1/Q2's non-group separator/declaration enforcement belongs here.
A group refusal cannot prove the ordinary `s=x cmd` split was fixed.
`bug-declaration-names-unvalidated` retains its independent identifier
validation work. None of these tasks needs a new policy decision.

Found 2026-10-27, while porting F1+F4 (batch-531 B28, journal 537). F1+F4
made a header `{` end a declaration, so `if int n=$(cmd) {` is DD#88's SB4025
(specs 71.6–71.8). It left the statement level alone (71.9). These are the
questions it did not decide then. The following tables are historical:
results were the same at `8e28c61` and with F1+F4, in both backends.
They were not rerun for the policy recording.

### Historical Q1: implicit statement splitting

| Id | Program | Result |
|---|---|---|
| sp1 | `s=x { echo hi }` then `echo "$s"` | `hi`, `x` |
| d3 | `s=x /bin/echo hi` then `echo "$s"` | `hi`, `x` |
| sp5 | `{ /bin/echo z } /bin/echo a` | `z`, `a` |
| bc.g6 | `echo a { echo z }` | `a`, `z` |
| csfx.19 | `{ /bin/echo hi } 2 >/dev/null` | SB5011 1:18 "command '2' not found — use absolute path or define as function" |

bc.g6 was pinned as a decision, but its source group is now retired,
as are sp1 and sp5. The earlier filing incorrectly called d3 unrecorded:
DD71 explicitly permitted `a=foo bar` to split after the binding. DG
has now retired that permission, not merely corrected an undocumented
accident. d3 must reject; its semicolon/newline-separated twin remains.
DD53's `/usr/bin/env` conversion remains the per-command environment
form, not the meaning of either sash binding sequence.

The split also happens after `return`, and there it deletes code:

| Id | Program | Result |
|---|---|---|
| rt1 | `fn f { return { /bin/true } }` then `if f { echo y } else { echo n }` | SB4037 1:1 "function `f` is used as a predicate but does not `return` on every path" |
| rt2 | `fn f { /bin/true; return { echo x } }` then `f` then `echo "rc=$?"` | `rc=0`; `x` is never printed |
| rt3 | `fn f { return /bin/true { echo x } }` then `if f { echo y } else { echo n }` | SB4037 1:1, same |

In rt1 and rt2, `return` takes no value and the group becomes the next
statement, which no path reaches. rt2 behaves as `return; { echo x }` does
(rpt7), and neither draws a diagnostic. In a predicate the group is the last
statement, so SB4037 fires, but it blames a missing `return`, not the `{`.
DD#74's table of producers has no `{ … }`. The disambiguation in
`parse_return` (`[[`, `((`, `!` or a non-numeric word is a producer; a
numeric word, a `$`-expansion or the end of the statement is a value) does
not say what a `{` is, so the value form takes it as the end. For comparison,
`return if …` is SB4014 (`q-compound-as-condition`, c9), and `return ! { … }`
returns the group's status (bng.23). rt3 is the plain Q1 split after a
producer.

### Historical Q2: declaration/assignment asymmetry

| Id | Program | Result |
|---|---|---|
| sp8 | `int n=1 { echo hi }` then `echo "$n"` | SB4014 1:1 "expected variable name in declaration" |
| sp9 | `array a=(x) { echo hi }` then `echo "${a[0]}"` | SB4014 1:1, same |
| d2 | `int n=1 /bin/echo hi` | SB5050 three times: `n`, `/bin/echo` and `hi` are declared as variables |
| d1 | `int n=1 m=2` then `echo "$n$m"` | `12` |

The original question compared declaration splitting with sp1. Group
retirement now settles sp1/sp8/sp9 as refusals, with advice about the
block rather than the valid variable name. Separation before an
otherwise legal non-group statement now follows the uniform boundary
rule. d1 remains valid; d2 needs a separator to run a command and must
not bind a path as a name. The independent name guard belongs to
`bug-declaration-names-unvalidated`.

### Q3. Settled: conditional-block sugar, not command-list grouping

DG's 2026-10-30 group retirement and 2026-11-09 symmetric shorthand
choice resolve this question. The old measurements below are retained,
not rerun. bc.g4 remains as `if ! /bin/false { echo a; echo b; }`;
bc.g5 and g4b require group refusals. g4a and g4c are conditional
bodies whose inner lists are separate, just as in an explicit `if`.
DD95 still forbids a mixed list; it does not forbid different lists
inside a body. Neither old option was selected as framed.

DD#95: "There is no grouping construct for command lists, and none is
wanted: a list that needs grouping is asking to be an `if` or a dispatch",
and "`( … )` is SB4014 and `{ …; }` is SB4027 there". Historical results:

| Id | Program | Result |
|---|---|---|
| bc.g4 | `/bin/false \|\| { echo a; echo b; }` | `a`, `b` |
| bc.g5 | `{ echo z }` | `z` |
| g4a | `/bin/false \|\| { /bin/true && echo y }` | `y` |
| g4c | `/bin/true && { /bin/false \|\| echo y }` | `y` |
| g4b | `{ /bin/false \|\| /bin/true } && echo y` | SB4023 1:1 |

bc.g4's old comment called a group the only multi-statement spelling.
The accepted contract now names the explicit `if` counterpart.
The earlier reading of g4c as mixed-list grouping is superseded: its
braces delimit an `if` body, so its operators are in separate lists.

Filed under DG's rule for the port that questions and uncovered cases become
backlog tasks.

## Implementation evidence

The 2026-11-09 read of `sash/src/parse/parser.rs` at `b6c26a9a` explains
the asymmetry without a new runtime measurement. `parse_assignment`
returned one Assignment after one RHS (2620-2678); the outer loop
skipped zero or more terminators (125-139,229-233). `parse_declaration`
instead owned a list (1157-1173,1220-1229), including bare declarators
(1329-1330). The policy selects no particular AST representation or
implementation strategy.

Take a fresh fixture census when implementation starts. Pin one/many
bindings, typed/untyped lists, ordered RHS effects, newline/semicolon
separation, final statements before enclosing closers/EOF, and valid
self-delimiting forms followed by ordinary statements. Pin operand
closers that must not supply a whole-statement boundary. Use meaningful
mutations, dispatch or attest forms rather than relying on an unused
test that another rule already rejects.

For adjacency, cover ordinary pairs, a capture as the last binding
after a clean prefix, multiple captured subjects followed by later
attests in either order, and a clean binding intervening before attest.
Cover declarations and plain lists, the legal `:!` alternative, clean
captures and whole-collection attest-all. Ensure later attests cannot
borrow adjacency from a list container or from a preceding attest.
Preserve signal deferral only across actual legal resolution pairs.

Check converter-generated binding/attest sequences and preserve DD53's
explicit `/usr/bin/env` conversion. Do not silently reorder input effects
to manufacture adjacent capture/attest pairs; use the existing conversion
diagnostic workflow where the ordering cannot be preserved.

An earlier group/name/result-use error is not proof of this guard.
Use valid used names, legal command heads and legal bodies; assert the
distinguishing diagnostic and execute the preserved programs in both
backends. Independently disable the separator and adjacency enforcement
to prove their controls. Allocate fresh ids above database history for
changed contracts; predict additions, retirements, flips and totals
only from that actual census.

`return {`, `return ! { ... }`, rt3, sp1/sp5/bc.g6 and sp8/sp9 remain
the group-retirement owner's refusals. csfx.19 is also group-only;
neither preserving its old unknown-command error nor rejecting its
group proves the legal whole-statement boundary rule. Update the
citations in `at_declaration_end` and the comments of errors.sh 71.9
and csfx.19 to these owners and rules when implementation lands.
No separator diagnostic number is selected by this recording.

Probes: `files/port-f1f4-probes.tar.gz`, directories `bk/` (sp1, sp5, sp8,
sp9), `sq2/` (d1–d3), `bl/` (g4a–g4c, cs19) and `wr2/` (rt1 is `rpt`, rt2
is `rpt8` with its two-line twin `rpt4`, rt3 is `rpt3`, and `rpt7`).
