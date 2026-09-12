---
title: Remove implementations of retired language surfaces
status: done
priority: P2
kind: work
depends: []
gates:
  - "no codegen path implements `echo -n`, `mapfile`/`readarray`, or the `read` flag set"
  - "SB5059 and SB5060 still fire, with their current wording — recognition for diagnosis is kept, implementation is not"
  - "`read -a a` / `read x y` do not silently become literal words in any backend"
---

# Remove the codegen implementations of DD#99-retired surfaces

## DG's ruling (2026-09-12)

Retired surfaces should not keep working implementations. The validator
refuses them; the code behind them is unreachable from any program that
compiles, and it is live enough to pass tests, which makes it look supported.

## What is actually still implemented

Verified end-to-end, not inferred: `echo -n hi` and `mapfile -t l` are both
rejected by the real compiler (SB5059, SB5060), yet both compile and run
correctly when the validator is bypassed — `codegen_tests::echo_n_no_newline`
and `mapfile_basic` pass today.

| surface | live sites |
|---|---|
| `echo -n` | `codegen/commands.rs::echo_parse_flags:48`, `emit_c.rs::emit_echo:2183`, janet equivalent |
| `mapfile` / `readarray` | `ssa.rs::BuiltinCmd::from_name:195`, `transform/statements.rs::is_bulk_read_cmd:344`, `transform/stmt_handlers.rs::is_mapfile_last:644`, `type_check.rs::check_command:1179` |
| `read` flags | `ir.rs::classify_read_word:766` and the whole `ReadFlags` struct — still parses `-a`, `-d`, `-f`, and accepts `-r` as a no-op |

## The one constraint

**The diagnostics must keep firing.** SB5059 and SB5060 name these surfaces
precisely, and DD#99's whole retirement story depends on an author who writes
`mapfile` getting told what to write instead. So recognition has to survive;
only the lowering behind it goes. Whoever does this should check whether
`classify_read_word` is what feeds the read-flag diagnostic before deleting it
— if it is, it stays and only its consumers go.

## Why it matters beyond tidiness

While the flags parse but no longer act, they degrade to **literal data**:
`read -a arr` yields a one-element array containing the string `-a`, and
`read -d: -r word` in a `while` consumes nothing and spins forever. Validation
is the only thing standing between that and a user. Removing the half-alive
implementation removes the failure mode, not just the dead code.

## Sequencing

This is one of the "other code changes to the compiler" that
`rust-tests-hang-on-retired-surfaces` is deliberately sequenced *after* — doing
it first shrinks that migration, since 5 of its 16 problem fixtures exist only
to test surfaces removed here.

## Found

2026-09-12, backlog review. Surfaced by measuring which retired-surface Rust
tests still pass rather than assuming they were all stale.

## DONE 2026-09-12

Landed with a 3-mode sweep at **5304 passing / 41 failing / 560 skipped**,
**zero non-janet failures**, and a rowid-split flip check showing **only the 42
new `rt99.*` rows and zero flips**. Pinned by `spec/commands.sh` `rt99.1`–
`rt99.14` (plan 74 → 88).

### Gate 1 was wrong — `echo -n` is not a retired surface

The filing's first gate is **unachievable as written, and should not have been
written.** `transform/cmd_handlers.rs::rewrite_write_as_echo` deliberately
lowers the *supported* `write` builtin to an SSA-level `echo -n`; the code
carries the comment "`-n` is now purely internal — the source spelling is
SB5059." `codegen/commands.rs::echo_parse_flags` and the `emit_c.rs` equivalent
are therefore the **implementation of `write`**, not a retired surface. They
stay. `rt99.10` pins that the *source* spelling is still SB5059 and `rt99.11`
pins that `write` still suppresses the newline.

This is the ninth filing in this batch found materially wrong on re-measurement.
A filing is a snapshot; nothing ages it.

### The open question, answered

The filing asked whoever did this to "check whether `classify_read_word` is
what feeds the read-flag diagnostic before deleting it." **It is not.**
`validate.rs::check_readline_arity` (SB5057) does its own word scan and never
consults the flag classifier, so the whole flag apparatus was free to go.

### What was removed

| layer | removed |
|---|---|
| `ir.rs` | `ReadFlags` → `ReadTarget` (only `var_names` survives); `classify_read_flags` → `readline_targets`; `classify_read_word`, `take_read_str_flag`, `take_read_byte_flag` |
| `ssa.rs` | `SsaInst::Mapfile`, `SsaInst::ReadLineMulti`, `ReadLine.delim`, `ReadArray.delim` |
| `transform/` | `try_fill_array`, `try_handle_mapfile`, `parse_mapfile_flags`, `mapfile_separator`, `is_bulk_read_cmd`, the unreachable SB5015/SB5016 transform diagnostics |
| `codegen/` | `handle_mapfile` (111 lines), `emit_readline_multi`, `ReadSplitRefs`, `emit_read_last_var`, `emit_read_field_split`, `emit_read_line_call`, three `ExternFuncs` fields |
| `emit_c.rs` | `emit_readline_multi`, `emit_readline_last`, `emit_readline_field`, `emit_mapfile`, three dead `extern` decls |
| `emit_janet.rs` | `parse_mapfile_consumer`, `parse_read_consumer`, `read_target_vars`, `field_sep_of`, `emit_lastpipe_read`, `janet_sep_str`, `janet_split_fields`, `janet_read_bind_field` |
| `sash-rt` | `sash_read_delim`, `sash_split_first_word`, `sash_split_field` (+ the two case-first symbols — see the sibling task) |

`ReadLine.delim` was `None` at all three producers and `ReadArray.delim` was
`Some(0)` at its one producer, so both fields were constants wearing a
parameter's clothes.

### Recognition really does have to survive — SB5011 proved it

Deleting `BuiltinCmd::Mapfile` from `from_name` broke gate 2 immediately and
non-obviously: `resolve.rs::is_resolvable_external` classifies a head as
external precisely when `BuiltinCmd::from_name(name) == Other`, so `mapfile`
became a PATH lookup and reported **SB5011 "command not found"** during
resolution — preempting SB5060 in the exact way `retired_bulk_read_diag`'s own
doc comment warns against.

`BuiltinCmd::Mapfile` is therefore **restored as a recognition-only variant**
with a doc comment saying so: no lowering, no `SsaInst`, no dispatch arm. This
is the shape the filing's second gate actually requires, and the same shape
`DeclKeyword::Export` has after the sibling `drop-removed-builtin-table` task.

### A pre-existing janet bug, found and fixed: lastpipe `| read ARRAY`

`/usr/bin/printf "p\0q\0" | read a` printed `0=97 1=0` in janet against
`0=p 1=q` native. `detect_lastpipe` classified on `BuiltinCmd::from_name`
alone, which maps **both** `read` and `readline` to `BuiltinCmd::Read`, so an
array target got scalar-string semantics and `${a[0]}` subscripted a *byte*.
`Lastpipe` is now split into `Readline { name }` / `ReadArray { name }`, keyed
on the literal command name, with the array form splitting on NUL and eliding
the single trailing empty (terminator semantics, matching
`sash_split_into_array`). Pinned by `rt99.14`, which fails on the pre-change
compiler.

This also closes **`janet-read-a-element-count`** outright: the "one element too
many" symptom was `len` counting bytes of a scalar. `spec/taint.sh` `tl.8` is
un-gated for janet.

### A second pre-existing janet bug, found and fixed: `write` forked `write(1)`

`emit_janet.rs` had **no `BuiltinCmd::Write` arm at all** — it had never had
one — so `write a` fell through to the general-command path and **forked the
external `/usr/bin/write`**, the `wall`-style terminal-messaging program. A
language builtin silently became someone else's binary. `emit_janet_echo` now
takes a `newline: bool` and `Write` routes to `prin`; `write` is added to
`janet_cmd_defaults_success`. Pinned by `rt99.11`, which fails on the
pre-change compiler.

Both janet bugs are the batch's recurring class again — **a value that could
not be resolved resolved to a plausible constant instead of failing.**

### Left in place deliberately

`mapfile`/`readarray` remain in `validate.rs`'s `BUILTINS` list and in the
taint name-lists. Those are recognition, not implementation, and removing them
reintroduces the SB5011 preemption above.

### Downstream

Shrinks `rust-tests-hang-on-retired-surfaces` as predicted: the `read -d`,
`read -f`, `mapfile_basic` and `${x^}` fixtures are gone. `codegen_tests.rs`,
`taint_spec_tests.rs` and `brush_compat_tests.rs` still carry others.
