# 075 — scan_ansi_c_quoted extraction

## Goal

Reduce `parse/lexer.rs::scan_ansi_c_quoted` (CC 30) to a short scan
loop. Per audit: extract `simple_escape(byte) -> Option<char>` plus
share hex/octal scanners (pattern #7).

## Approach

Four lifts:

1. `simple_escape(b: u8) -> Option<char>` — pure 12-arm table for the
   named escapes (`\n`, `\t`, `\r`, `\a`, `\b`, `\e`/`\E`, `\f`,
   `\v`, `\\`, `\'`, `\"`). Returns `None` for octal/hex/unknown so
   the caller can fall through. Tagged `// cyclo-allow:` —
   arms == CC by construction, same shape we already allow
   (cf. `file_test_op_code`, `dispatch_statement`).
2. `consume_ansi_c_escape(&mut self, out: &mut String)` — the body
   of the `\` branch: try `simple_escape`, else dispatch to octal /
   hex / literal-passthrough.
3. `scan_ansi_c_octal(&mut self) -> u8` — at most three octal digits
   after `\0`, stops on first non-octal byte or EOI. Uses
   `wrapping_mul` / `wrapping_add` to keep clippy happy on the
   intentional 0–255 truncation (matches original `val * 8` byte
   overflow semantics).
4. `scan_ansi_c_hex(&mut self) -> u8` — same shape for `\xHH`, two
   digits, hex_digit-or-break.

The parent shrinks to a single loop: literal-or-`\` dispatch.

The audit suggested sharing hex/octal scanners with `scan_double_quoted`,
but that scanner only recognises four escapes (`\"`, `\\`, `\$`, `` \` ``)
and `\<newline>` line-continuation — no octal/hex. So the helpers stay
local to `scan_ansi_c_quoted`.

## Result

| function | CC |
| --- | --: |
| `scan_ansi_c_quoted` | 30 → **6** |
| `consume_ansi_c_escape` | 5 |
| `scan_ansi_c_hex` | 7 |
| `scan_ansi_c_octal` | 4 |
| `simple_escape` | 13 (pure table; `cyclo-allow`'d) |

## Verification

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib` — 12/12
- `spec/run.sh` — 291/291

## Notes

- Behaviour-preserving. Octal digit check went from
  `>= b'0' && <= b'7'` to `(b'0'..=b'7').contains(&b)` — same
  range, clippy-preferred form.
- The `\0` arm in the original `match` was the only place that
  advanced past the `0` and then read digits; the helper preserves
  that exact protocol.
- The "other" arm continues to push the literal `\` followed by the
  byte — keeps bash-compatible passthrough for unknown sequences.
