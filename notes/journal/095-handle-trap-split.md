# Journal 095 — handle_trap shape split + signal_name_to_num

## Goal

Fifth Phase 10 target. `handle_trap`
(`transform/cmd_handlers.rs:151`, CC 24) — C-verdict from audit
row 223. Recipe: "Multi-shape with signal-name→number table inline.
Extract `signal_name_to_num()`."

## Pattern

`handle_trap` handled three distinct shapes in one function:

1. `trap 'body' EXIT` — parse body, store on state, emit before
   every Return terminator.
2. `trap '' SIG` / `trap - SIG` — emit TrapSignal sigil with
   SIG_IGN (handler=1) / SIG_DFL (handler=0).
3. Anything else — emit as regular command (fallback).

Embedded in shape 1 was a quote-stripping helper for the body
literal (`'…'` and `"…"` → inner bytes). Embedded in shape 2 was a
6-row signal-name → kernel-signum table. Both were anonymous
inlined logic.

## Refactor

Five helpers extracted, each owning one rule:

| helper | what it owns | CC |
| --- | --- | --: |
| `signal_name_to_num(signal)` | 6-row table + wildcard fallback | 8 |
| `is_ignore_reset_body(body_str)` | the 4-OR `''/""/empty/-` test | 4 |
| `strip_outer_quotes(s)` | shared single-quote/double-quote strip | 5 |
| `handle_trap_exit(body_str, state)` | EXIT shape: strip + parse + store | 3 |
| `handle_trap_ignore_reset(body_str, signal, ssa, block)` | SIG shape: lookup + emit | 3 |

The dispatcher itself is now a clean 3-shape guard:

```rust
if c.args.len() >= 2 {
    if signal == "EXIT" || signal == "0" {
        handle_trap_exit(&body_str, state);
        return block;
    } else if is_ignore_reset_body(&body_str) {
        handle_trap_ignore_reset(&body_str, &signal, ssa, block);
        return block;
    }
}
// fallback: emit as regular command
```

`signal_name_to_num` is reusable for any future code that needs to
go from a signal name to a kernel number; `handle_signal` (the peer
function below) parses its argument as `sig_str.parse::<i64>()` so
it doesn't benefit, but a future `kill` builtin would.

## Result

| function | before | after |
| --- | --: | --: |
| `handle_trap` | 24 | 6 |

All 5 new helpers under the audit band. `handle_trap` dropped well
below the visible top-5.

## Insight

This is the cleanest of the Phase 10 hotspots so far because the
recipe pre-identified the table. The audit (5-model consensus,
3-pass) called out "Multi-shape with signal-name→number table
inline" — that single sentence implied three structural moves:
(a) name each shape, (b) extract the table, (c) name the
guard-predicate (`is_ignore_reset_body`). Following the recipe
literally produces a clean refactor with no surprises.

Worth noting that `strip_outer_quotes` is the only helper here
that's likely to be needed elsewhere — quote-stripping is a
recurring pattern in trap/eval-like contexts. It's now available
in the `transform::cmd_handlers` module if needed.

## Gates

- `cargo build --release` clean
- `cargo clippy -D warnings` clean
- `cargo test --lib` 12/12
- `spec/run.sh` 291/291

## Files touched

- `sash/src/transform/cmd_handlers.rs` — extracted 5 helpers;
  `handle_trap` shape-dispatcher only.
- `notes/tasks/r47-abstraction-audit.md:223` — annotated.
