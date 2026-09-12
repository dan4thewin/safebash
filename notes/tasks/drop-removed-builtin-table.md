title: Drop the parser removed-builtin table (no reserved names)
status: ready
priority: P3
kind: task
depends: []
gates:
  - "`rejected_keyword_msg` / `try_rejected_keyword` are gone"
  - "`export`, `declare`, `typeset`, `readonly`, `select` give the ordinary unknown-command error (SB5011)"
  - "a user-defined function named `select` / `export` / `shift` is callable"
  - "`source` / `.` are still rejected — SB4001, dynamic execution"
  - "specs cov.11 and cov.24 assert the new codes"

# Drop the removed-builtin keyword table

**DG, 2026-09-12:** "drop the removed-builtin table altogether. error just
as for any other unknown token in cmd position."

`sash/src/parse/parser.rs:1697–1748` holds `try_rejected_keyword` +
`rejected_keyword_msg`, a table that intercepts six bash words in command
position and emits a bespoke diagnostic instead of letting them fall through
to ordinary command resolution:

    source / .          SB4002   "a sash program's behavior is fixed at compile time"
    export              SB5001   "use /usr/bin/env NAME=value …"
    declare / typeset   SB5001   "sash declares with int / const / array / map"
    readonly            SB5001   "sash declares read-only values with const"
    select              SB5001   "sash has no `select`"

Two costs. It **reserves the names** — a user-defined `select() { … }`
parses but is unusable at the call site — and it makes the unknown-command
story non-uniform: `shift` (also removed) gets the generic SB5011, these six
get something else.

After this change every one of them lands on the same SB5011 the rest of the
language already uses: *"command 'X' not found — use absolute path or define
as function"*.

## Nothing is lost — verified before filing

- **`source` / `.` stay rejected.** `validate.rs::check_dynexec` (line 1014)
  independently rejects `eval`, `source`, `.`, `command`, and `coproc` with
  **SB4001** *"`X` is rejected — sash forbids dynamic command execution"*.
  The table's SB4002 arm is redundant with it. Dropping the arm changes the
  code and the wording, not the rejection — and arguably improves it, since
  one canonical reason replaces two overlapping ones.
- **The `export` path in `validate.rs` is already dead.** `validate_declaration`
  (line 1067) has an SB5001 arm gated on `DeclKeyword::Export`, but **no site
  constructs `DeclKeyword::Export`** — `declaration_axes` only ever produces
  `Declare` and `Readonly` (from sash's own `int`/`str`/`const`/`array`/`map`).
  The parser table rejected `export` before it could reach a Declaration, so
  that arm has never been reachable.
- **The lexer arms are no-ops.** `lexer.rs::classify_word_token:1706–1711`
  maps each of the six to `Tok::Word(same_string)` — byte-identical to the
  `_ => Tok::Word(word)` fallback directly below. They are explicit
  documentation, not behaviour.

## Scope

1. Delete `try_rejected_keyword` + `rejected_keyword_msg` and the call site
   at `parser.rs:1705`.
2. Delete the dead `DeclKeyword::Export` arm in `validate_declaration`.
3. Delete the six no-op arms at `lexer.rs:1706–1711`.
4. Update the two specs that assert the old codes:
   - `spec/coverage.sh` cov.11 — `source`/`.` → **SB4001** (was SB4002)
   - `spec/coverage.sh` cov.24 — `export` → **SB5011** (was SB5001)
5. Update `sash/src/parse/tests.rs:61`, which asserts SB5001.
6. Add a spec that a user-defined function named `select` is callable — the
   capability this change restores.

This also closes item 5 of `dd75-args-model-polish`: `shift` needs no
special-casing, because SB5011 becomes the uniform answer rather than the
odd one out.

## Out of scope

Retiring the now-unused `DeclKeyword::Export` **variant** itself. It is dead,
but removing it ripples into `emit_janet.rs:403` and
`emit_janet_map.rs:262` (`sanitize_flags`). Leave the variant; delete only
the unreachable `validate.rs` arm. File separately if the dead variant is
worth chasing.

## Note

SB4002 may be left with no remaining emitter after this. Check before
assuming the code is still live.
