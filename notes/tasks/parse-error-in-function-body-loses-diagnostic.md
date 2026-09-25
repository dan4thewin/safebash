---
status: open
priority: P2
depends:
gates:
---

# A parse error inside a function body loses its own diagnostic

Found 2026-09-25 while fixing `return <reserved word>` (journal 410). **Not
caused by that change** — it is the pre-existing recovery behaviour, and it was
only visible because I ran the same input at top level and inside a function
to get a baseline.

## The symptom

    $ cat p.sh
    echo a |
    $ sash p.sh
    p.sh:1:8: error [SB4056] `|` has no right-hand side — a pipeline needs a
                             command on both sides

    $ cat p.sh
    f() {
    echo a |
    }
    f
    $ sash p.sh
    p.sh:3:1: error [SB4014] expected expression

The same program, one nesting level down, reports a **different** error, at the
**wrong line**, naming the orphaned `}`. SB4056 — the accurate, specific
diagnostic, with the right span and the right advice — is gone entirely.

## Why

`parse_body_until` propagates the `Err` with `?`, so the enclosing
`parse_function` is abandoned mid-body. The error reaches
`parse_top_level_item`, which pushes it and calls `recover()` — and `recover()`
skips only to the next newline. Parsing resumes at the dangling `}`, which is
not a statement, so a *second* diagnostic is produced. At top level there is no
enclosing construct, so only the real error appears.

Both diagnostics are reported when the inner one survives, but here the inner
one is the one that got swallowed — worth confirming exactly where, since the
double-report case (below) shows the machinery *can* emit both.

## Evidence it is the recovery, not the diagnostic

`return if` (the journal 410 fix) reports **both**:

    p.sh:2:8: error [SB4014] return: expected a status producer (…)
    p.sh:3:1: error [SB4014] expected expression

Same shape, same nesting, and the real error survives. So the machinery can
carry the inner diagnostic out; the `echo a |` case loses it somewhere more
specific than "errors inside functions are lost". Find that before designing a
fix.

## Why it matters beyond tidiness

The follow-on is not merely noisy, it is *misleading*: it points at a line the
user did not get wrong and names a condition (`expected expression`) that is
not the problem. A user debugging the function-body case is sent to the closing
brace.

It also means **spec assertions on a parse-error code can be vacuous inside a
function body** — `$err == *SB4014*` passes on the follow-on alone. The
journal 410 specs assert the message text for exactly this reason; anything
else asserting a bare parse-error code in a nested context should be audited.

## Suggested shape (not decided)

- Have `recover()` understand block nesting, or
- have `parse_body_until` catch the `Err`, record it, and resynchronise to the
  body's stop token so the `}` is consumed normally.

The second keeps recovery local to the construct that knows where its body
ends. Either is a change to diagnostic behaviour across the whole parser, so it
wants a before/after audit of parse-error output — DG's call.
