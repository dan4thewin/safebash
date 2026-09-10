# 173 — argv/table index-var read is_int-total + `$@[i]` ICE + atoi audit

Follow-through on the P1 discussion (DG: the "arrays/index-vars are always
Str-slots" invariant is a convention tests can't prove — and `$@[i]` proved it
by ICE-ing). Three pieces.

## 1. Index-var read is `is_int`-total (`codegen/values.rs`)

`lower_argv_index_value` (ArgvIndex) and the shared `lookup_atoi_var`
(TableValue/TableKey iteration) both did `str_ptr()` + `atoi` on the index var —
assuming it's a string. That's a convention between the loop-lowering (which
emits the counter `__fi` as a string) and this read, *not* a type-level
guarantee: an int `__fi` would `str_ptr()`-panic. Merged both into
`index_var_to_int`, which dispatches on `is_int` — an int slot is read directly,
a string slot is `atoi`'d. Removes the `str_ptr`-on-int panic risk and frees a
future int loop counter. (Defensive today — `__fi` is still a string — so no
spec exercises the int branch; it mirrors the existing `lower_var_as_int`.)

## 2. `$@[i]` / `$*[i]` ICE → clean SB5031 (`validate.rs`)

`$@[0]` reached codegen as a subscript on an "array" named `@` and panicked
(`ICE: undefined array: @`, `find_array_slot`) — `flag_if_unknown` skips
non-plain-identifiers, so `@`/`*` slipped past the undeclared-read check.
`check_subscript_index` now rejects a `Subscript` whose name is `@`/`*` (the
positional splat isn't subscriptable) → SB5031 pointing at `$1`..`$N` /
`${args[i]}`. spec/subscript.sh 80.20-21. (The braced `${@[0]}` is a different
node that compiles to empty — noted, niche, no ICE.)

## 3. atoi audit (DG: "see that the only atoi is within string interpolation")

Walked every `atoi` site, native + C:

- **native** — all string→int, none atoi an already-int value. Int-cell reads
  (`Subscript`/`IntCell`) go through `lower_disposition_as_int` and read the i64
  directly; `lower_var_as_int` and the new `index_var_to_int` are `is_int`-
  guarded; `arrlen`→`table_len`, arith/ternary compute directly, `Literal`→
  `parse::<i64>` (not atoi), int intrinsics→`int_intrinsic_i64`. The remaining
  atoi's are on genuinely-string sources: string vars/cells, the `_ =>` fallback
  (string-producing values only), `${x:-N}`/`${x:?m}` source strings, captured
  stdout into an int var (`int x=$(cmd)`), the `$!` pid. Clean.
- **C** — `emit_c.rs:763` (captured stdout → int var) is target-`is_int`-guarded.
  `emit_c.rs:1244` (argv index) atoi's the string `__fi` **unguarded** — audit-
  legitimate today (string→int) but not `is_int`-total like native. Filed
  `c-argv-index-isint-parity` (parity gap, not a live bug — `__fi` is a string).

Conclusion: after piece 1, every atoi is a string-value→int coercion; nothing
atoi's a value already known to be int.

## Verification

Full `SPEC_MODES="native c janet"` sweep vs HEAD (`4bdd88d`): 0 passing→failing,
0 gone, 6 new passing (80.20-21 ×3); native gate 0. Dynamic-for over `$@` and
`for x in @arr` both intact.
