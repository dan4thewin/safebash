title: DD#41 — Sash-native array syntax
status: implemented

1. Declare collections with `array` or `map`; use `@name` to splat their values.
2. Collection intrinsics take bare collection names: `len` counts elements,
   `keys` reads map keys and `join` joins values (DD75/DD100).

---

`array arr=(...)`, `map m=(...)` as declaration forms (implemented).
`@arr` for ~~`"${arr[@]}"`~~ in iteration ~~(designed, not fully wired)~~.
`len arr`, `keys arr`, ~~`join @arr`~~ as compiled builtins ~~(designed)~~.
DD75 makes `keys` map-only; DD100 requires `join arr`, not a splat operand.
