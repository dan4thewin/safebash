title: Remove implementations of retired language surfaces
status: ready
priority: P2
kind: work
depends: []
gates:
  - "no codegen path implements `echo -n`, `mapfile`/`readarray`, or the `read` flag set"
  - "SB5059 and SB5060 still fire, with their current wording — recognition for diagnosis is kept, implementation is not"
  - "`read -a a` / `read x y` do not silently become literal words in any backend"

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
