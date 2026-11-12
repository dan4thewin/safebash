# 270 — The native batch, run unattended

DG: **"do the whole batch unattended."** Thirteen items queued by the
backlog review (journal 269), worked in priority order, each ending in a
3-mode sweep, a commit, and a push. This is the record of what the batch
actually cost and what it turned up, which was not what the ledger said it
would be.

## The shape of it

Fifteen commits, `cc9aea5..999e014`. Eleven of the thirteen landed; two are
blocked on DG decisions that the work itself surfaced.

| # | item | outcome |
|---|---|---|
| 1 | `bug-getenv-status-unvalidated` (P1) | fixed — every value intrinsic now has an arity |
| 2 | `bug-file-test-n-diverges-from-bash` (P2) | fixed — plus the C backend was *stubbing file tests entirely* |
| 3 | `dd55-while-condition-redirect-divergence` (P2) | fixed — one shared condition-position predicate |
| 4 | `dd75-len-collection-only-strlen` | `strlen` added; `len` is collection-only |
| 5 | `dollar-lt-file-forks` (P2) | **blocked** — heap-capacity decision |
| 6 | `cli-usage-and-unknown-flags` (P2) | fixed — one usage string, a real `--help` |
| 7 | `getenv-presence-handling` | settled as **DD#101** |
| 8 | `bug-convert-hash-array-conflation` (P2) | **blocked** — `strlen` cell spelling |
| 9 | `drop-removed-builtin-table` | dropped; unknown words error uniformly |
| 10 | `remove-retired-surface-implementations` | done — and the filing was wrong |
| 11 | `cleanup-stale-local-names` | done |
| 12 | `unary-test-ops-k-g-u` | done — parser-only, as filed |
| 13 | `dup-is-identifier-leading-digit` | done — **and it was not inert** |

## Nine of nineteen filings were wrong, and the batch found three more

Journal 269 opened with that number from re-measuring the backlog. Working
the batch added to it rather than settling it.

`remove-retired-surface-implementations` asserted as its first gate that "no
codegen path implements `echo -n`." That gate is unachievable, and should
never have been written: `rewrite_write_as_echo` deliberately lowers the
**supported** `write` builtin to an SSA-level `echo -n`, with a comment
saying so. `echo_parse_flags` is the implementation of `write`. The filing
had confused a retired *source spelling* with a live *internal lowering*.

`janet-read-a-element-count` blamed "the trailing newline counted as a third
empty record." The real cause was that janet's lastpipe bound an array
target as a **scalar string**, so `len` was counting bytes. The filing's
gate was also unwritable by then — it used `read -a`, which DD#99 retired.

`bug-convert-hash-array-conflation` was filed as a bug to fix and turned out
to be a **decision to make**, because two of its four rows need a spelling
for "the byte length of one array element" that the language does not have.

The pattern is consistent enough to state plainly: **a filing is a snapshot,
and nothing ages it.** Re-measure before trusting one. Nine of nineteen at
review time, three more during implementation.

## The bugs the batch found that nobody had filed

Every one of these came from probing the binary while checking something
else, not from reading code.

**Heap exhaustion was a silent SIGSEGV** (P1). The allocator returned a null
on exhaustion and the caller wrote through it. Fixed; `$(<file)` slurp was
also truncating at 64 KiB.

**The C backend was stubbing file tests.** Found while fixing `-N`: the C
backend did not implement the file-test opcodes at all. The `-N` divergence
was real but it was the smaller half of what was there.

**janet's `write` forked `/usr/bin/write`.** `emit_janet.rs` had no
`BuiltinCmd::Write` arm and had never had one, so `write a` fell through to
the general-command path and invoked the `wall`-style terminal-messaging
program. A language builtin silently became someone else's binary.

**janet's lastpipe `| read ARRAY` bound a scalar.** `detect_lastpipe` keyed
on `BuiltinCmd::from_name`, which maps **both** `read` and `readline` to
`BuiltinCmd::Read`, so an array target got scalar-string semantics and
`${a[0]}` returned a *byte*.

**`9x =~ s/a/b/` compiled to a working binary.** `parser.rs` carried its own
`is_identifier` that never checked the first character — contradicting its
own doc comment — so `9x`, `0`, and `123` all passed as variable names. Bash
answers "command not found" to every one of them. The filing said explicitly
"whether that is presently reachable is exactly what needs measuring — do
not assume it is inert," and it was right to.

## The recurring class, now at eleven

Every bug above is the same shape: **a value that could not be resolved
resolved to a plausible constant instead of failing.** A null heap pointer
treated as an address. A missing opcode treated as `false`. An unknown
builtin treated as an external program. An array treated as a string. A
digit treated as a name.

The fix is always the same too: **one shared definition, and fail loudly
when you cannot resolve something.** Three of this batch's commits are
literally that — one shared condition predicate (DD#55), one shared operand
guard (`strlen`), one shared `is_identifier`.

Two-copies-of-a-predicate accounts for four of the eleven. That is worth
saying out loud: the duplication is not a style problem, it is where the
bugs live.

## Recognition is not implementation, and the difference is load-bearing

Removing `BuiltinCmd::Mapfile` looked obviously safe — DD#99 retired
`mapfile`, nothing lowers it, the name stays in `BUILTINS` for diagnosis.
The specs disagreed immediately: `mapfile -t l` started reporting **SB5011
"command not found"** instead of SB5060.

`resolve.rs::is_resolvable_external` classifies a head as external precisely
when `BuiltinCmd::from_name(name) == Other`. Dropping the variant made
`mapfile` a PATH lookup, and resolution reported not-found before validation
ever ran — the exact preemption that `retired_bulk_read_diag`'s own doc
comment warns against.

So the variant came back, documented as recognition-only: no lowering, no
`SsaInst`, no dispatch arm. It is the same shape `DeclKeyword::Export` has
after the removed-builtin table was dropped. **A retired surface needs two
things kept: the name, and the diagnostic. Everything between them goes.**

## DG's uniformity rule did real work

When the removed-builtin table was dropped, DG said: *"error just as for any
other unknown token in cmd position."* That is one line, but it settled three
separate questions later in the batch without another round trip.

It settled what happens to `9x=1` once `9x` is no longer a valid identifier
(falls to command position, errors there — no new diagnostic invented). It
settled that a PATH-shadowed name is not a special case. And it is why none
of the four cleanup commits added an error code.

This is the decision tree working: a stated principle applies to **all** its
consequences, not just the case that prompted it.

## The complexity gate was satisfied by refactoring, three times

The gate rejected commits twice in this batch and once in the prior one, and
each was fixed by extracting a helper rather than bypassing:

- `validate_stmt_ctx` (13/3) — extracted `validate_declaration_stmt`, mirroring
  the existing `validate_assignment_stmt`. Tripped because touching a *call
  site* counts as touching the function.
- `lower_pipe_while_read` (cog 16 → 0) — the fd repointing became
  `repoint_readlines_to_pipe` / `repoint_block_readlines`, and a `let`-else
  guard replaced the `if let` pyramid.
- the janet echo/write emitter — arg joining extracted to `janet_echo_arg`,
  so `emit_janet_echo` and `emit_janet_write` are three lines each.

`SKIP_COMPLEXITY_GATE=1` was not used.

## Where the suite ended up

| | start of batch | end |
|---|---:|---:|
| passing | 5194 | **5351** |
| failing | 41 | 41 |
| skipped | 566 | 567 |

**Zero non-janet failures throughout.** All 41 failures are janet, a
known-incomplete backend with ~54 filed gaps. Three of them are new
(`rt99.2`, `rt99.12`, `rt99.13`) and all three are the already-filed
`janet-read-family-ignores-input-redirect`; one janet test moved the other
way (`tl.8`, un-gated after the lastpipe fix).

Every fix in this batch was **fail-first verified**: stash the compiler
change, rebuild, confirm the new specs fail, restore. Four times that
verification changed what got written — a spec that passed on the old
compiler was testing the wrong thing.

## What DG still has to decide

Two, both surfaced by the work rather than by the filings:

1. **Heap capacity** — `$(<file)` needs a policy for a file larger than the
   arena. (A) grow, (B) fail loudly with a diagnostic.
2. **The `strlen` cell spelling** — `${#a}` in bash means "bytes of element
   0," and sash has no way to ask that. `${a[0]}` already parses as a cell
   and is rejected only because `classify_operand` buckets it as "not a
   name" — which is right for `len`/`join`/`keys` and an open question for
   `strlen`.

Both block exactly one batch item each and nothing else.
