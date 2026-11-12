title: "spec: cov.306–308 pin validate's SB4044, not the transform-time gate they describe"
status: open
priority: P3
kind: task
depends: []
gates:
  - "each of `--emit-ir`, `--manifest` and `--emit-c` is shown to reject a program whose ONLY diagnostic is raised by the transform"
  - "the new checks take fresh ids above the spec.db high-water mark for their prefix; cov.306–308 are retired or re-pointed, with the comment corrected"
  - "a reversion proof: making the alt-flag path skip the transform's diagnostics turns the new checks red"

## What

`spec/coverage.sh` (the loop after cov.305) says:

> cov.306-308: transform-time compile errors must gate EVERY backend, not
> just native. … pin that the alt-output flags still reject what native
> rejects. (journal 348)

The program it compiles under each flag is
`const int K=1; K=2; echo $K`. Validate rejects that program (SB4044,
`-c:1:16`) before the transform runs, on both the pre-port base
(`1309385`) and the batch-531 F13 tree. Journal 348 does not name the
diagnostic it meant. SB4044 was then raised in two places: validate's
spanned check, which this program hits, and `VersionState::define`'s
spanless transform backstop, reached only through channels validate did
not list (`read`, `mapfile`; `const-reassign-panic.md`). F13 moved every
channel into validate and turned the backstop into an ICE, so no
transform-time SB4044 exists any more. The three checks pass whatever the
transform does: they cannot fail for the reason their comment gives.

## Candidates

These programs are rejected only by the transform. Measured on the F13
tree, each one is rc 1 with its code under all three flags:

| Program | Code | Raised in |
|---|---|---|
| `fn f { echo hi }; f a` | SB5043 | `transform/cmd_handlers.rs` (`check_callee_arity`) |
| `fn f { echo hi; f }; f` | SB5037 | `transform/functions.rs`, `transform/stmt_handlers.rs` |
| `fn f { echo $1 }; str x=$(f); echo "$x"` | SB5042 | `transform/cmd_handlers.rs` |
| `fn f { echo "$1" }; array a=(x y); f @a` | SB5087 | `transform/cmd_handlers.rs` |

No `validate.rs` or `type_check.rs` site raises any of these four codes.
One program per flag is enough. Re-check the raise sites before choosing,
since a later family may move a check into validate.

## Notes

The ids reach `chk` through `_id=`, so the literal-id collision check does
not see them (AGENTS.md, the variable-id caveat). Any renumbering must also
pass the second collision check and the spec.db high-water mark.

Found during the batch-531 port (F13). Left unchanged there under the
port's no-digression rule.
