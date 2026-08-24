# 176 — VarSlot Pattern-A → exhaustive match (P1 of eliminate-ice-arms)

Continued the ICE-arms cohort. **P1 Pattern-A**: the codegen sites that
dispatched on `VarSlot::is_int()` and then reached through a *panicking*
accessor (`as_int()` / `str_ptr()` / `str_len()`) on the flag-checked variant.

## The shape (5 sites)

```rust
if sv.is_int() {
    … *sv.as_int() …          // panics if actually Str
} else {
    … sv.str_ptr() … sv.str_len() …   // panics if actually Int
}
```

The `is_int()` guard makes the accessor panic unreachable *from that site*, but
the type system can't see the coupling: the code still textually calls a method
that ICEs on the other variant, and a future third variant would fall silently
into the `else`.

## Fix — direct exhaustive match

Each site now matches the enum and binds the variant's fields:

```rust
match *sv {
    VarSlot::Int(v) => …,
    VarSlot::Str { ptr, len } => …,
}
```

Total by construction — a new variant fails to compile here instead of
misbehaving. Sites converted:

- `values.rs::lower_var_value` (var in string context)
- `values.rs::index_var_to_int` (array subscript index var)
- `values.rs::lower_var_as_int` (var in int context)
- `inst_handlers.rs::handle_regular_assign` (scalar assign)
- `dispatch.rs::emit_capture_end` (`$(cmd)` capture into a slot)

## Consequence — two accessors deleted (2 ICE arms gone)

After the conversion, `VarSlot::as_int()` had **zero** callers and
`VarSlot::is_int()` had zero callers; `VarSlot::as_str()` was already
`#[allow(dead_code)]`. All three are removed — deleting the
`panic!("ICE: expected int var, got string")` (as_int) and
`panic!("ICE: expected string var, got int")` (as_str) arms outright. This is
the "via-types" thesis in action: the panics didn't need a comment justifying
them, they needed to not exist.

## Kept + cited — str_ptr / str_len (Pattern-B)

`str_ptr()`/`str_len()` still have ~40 Pattern-B callers on slots the caller
has *established* are strings (positionals, arrays, read/capture targets, and
now the `VarSlot::Str` arm of a match). Those panics stay as the accessor's
contract boundary, with an `// INVARIANT:` comment on the `impl` block
explaining that the Int arm is unreachable from those sites and a hit means a
codegen path built a wrong-shaped slot (a compiler bug, not user input).

## Verification

- `cargo build`/`clippy --tests -D warnings` clean (deleting the dead
  accessors was *required* — they'd have tripped `-D warnings`).
- Behavior spot-check: int/string assign+read, var-in-arith, `${a[$i]}`→y,
  `int p=$(getpid)` capture, `$(( a[i] ))`/`${a[i]}` still correctly SB4030.
- Full `SPEC_MODES="native c janet"` sweep, latest-run-per-test diff vs base
  `3ddfc59`: **0 passing→failing, 0 gone, 0 new**. Native gate 0 failing.

Behavior-preserving (`match` ≡ the 2-arm `is_int` if/else) → no new specs.

## Cohort status

Done: P8 (join arity → SB5040), P4 (CheckedArithOp), P6 (case→bool), P2 (lazy
interner), P1 Pattern-B (index-var is_int-total), **P1 Pattern-A (this) +
as_int/as_str deletion**. Remaining: keep+cite pass for the P3 (undefined
var/array/index → validate SB5031) and P7 (should-have-been-lifted) sites, and
a reachability check on the "unimplemented/unsupported operator" panics
(tests.rs test ops, value_handlers.rs param-expansion op) before deciding
cite-vs-fix.
