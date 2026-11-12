title: "Harness: every diagnostic's suggested spelling compiles, as a standing gate"
status: ready
priority: P3
kind: infra
depends: []
gates:
  - "a harness triggers each diagnostic that suggests a spelling and compiles the suggestion"
  - "runs as part of the spec suite, so a message that rots fails a gate rather than reaching a user"
  - "the extraction reads rendered stderr, not source text"

## Context

`audit-diagnostic-suggested-spellings` found three instances of a diagnostic
recommending a form the compiler rejects (SB4035, SB5030, and the SB4030
string→int hint) and fixed them. That was a **manual pass**. The class will
re-open the next time the accepted surface moves, because nothing compiles a
message.

## Why this is harder than it looks

The obvious implementation — scan `sash/src` for backtick-quoted fragments in
message literals — **does not work**. It produced two confident false
positives in the audit: `` use `@{name}` `` reads in source as the invalid
braced splat `@{a}` (SB5033), but it is a `format!` placeholder that renders
as `@a`, which is correct.

The message a user sees is the only thing worth auditing, and it exists only
after interpolation. So the harness has to:

1. Hold a fixture per diagnostic that **triggers** it.
2. Capture stderr.
3. Extract the suggested spellings from the rendered text.
4. Substitute them into a program and compile.

Step 1 is the cost: it needs a trigger fixture for every diagnostic that
carries advice. Step 3 needs the messages to be regular enough to parse —
today they are not, and the fragments appear after "use", "write", "quote it
to …", "for a joined string", "for one element", and several one-offs.

## Approach options (for whoever picks this up)

- **(a) Convention + parser.** Settle one phrasing for suggestions, migrate
  the messages to it, then parse mechanically. Highest up-front cost, lowest
  per-message cost afterwards, and it improves the messages.
- **(b) Structured suggestions.** Carry them as data on `Diagnostic`
  (`suggestions: Vec<String>`) rather than baking them into the message; the
  renderer appends them. The harness then reads the field directly and never
  parses prose. Touches every construction site.
- **(c) Fixture table.** A spec-side table of `(trigger program, expected
  code, recommended program)` triples, maintained by hand. Cheapest to start,
  no source changes, but it is a second place to forget to update — the same
  weakness that produced the bug class.

(b) is the one that makes the gate structural rather than diligent, which is
the whole point. It is also the most invasive. **Worth DG's call before
anyone builds it** — it changes the `Diagnostic` type, which is a surface
decision, not an implementation detail.

## Note

`spec/types.sh` `hint.1`–`hint.6` and `spec/glob_splat.sh` `gs.36`/`gs.37`
are the current hand-written instances of the pattern the harness would
generalise. They are the right shape to copy: assert the message text **and
run the recommended form**.
