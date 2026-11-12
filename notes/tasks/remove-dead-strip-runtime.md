title: "remove dead sash_strip_prefix/sash_strip_suffix runtime + externs"
status: done
priority: P3
kind: cleanup
depends: [vet-var-modifiers]
gates:
  - "sash_strip_prefix / sash_strip_suffix removed from sash-rt (no remaining callers)"
  - "ExternFuncs.strip_prefix / .strip_suffix fields + their setup.rs declarations removed"
  - "emit_c.rs extern C prototypes for sash_strip_prefix/suffix removed"
  - "sash-rt rebuilds; native + c spec modes green (0 regressions)"

# remove the now-dead strip runtime functions

DD#65 dropped both the scalar `${var#p}`/`${var%p}` strip operators (commit
39519e5) and the per-element array `${arr[@]#p}`/`${arr[@]%%p}` forms
(follow-up). With both callers gone, `sash_strip_prefix` and
`sash_strip_suffix` have **no remaining users** anywhere in generated code.

They survive only as dead ABI decls (the `ExternFuncs` struct is
`#[allow(dead_code)]`, so the compiler doesn't flag them):

- `sash/src/codegen/types.rs` — `strip_suffix` / `strip_prefix` fields.
- `sash/src/codegen/setup.rs` — the `strip_suffix_sig` signature, the two
  `declare_function("sash_strip_{prefix,suffix}", Import)` calls, and the
  field assignments (~153-169, ~709-710).
- `sash/src/emit_c.rs` — the `extern i64 sash_strip_{suffix,prefix}(…)` C
  prototype lines (~130, ~135).
- `sash-rt` — the `sash_strip_prefix` / `sash_strip_suffix` runtime
  function definitions (and the C oracle's copies, if any).

## Notes

- Purely dead-code removal; no behavior change. Verify by rebuilding
  `sash-rt` (`cd sash-rt && cargo build --release`) and running native + c
  spec modes (c links `libsash_rt.a`).
- Keep `sash_default_value` (used by `${var:-…}` DefaultValue) and
  `sash_toupper`/`sash_tolower` (used by `$(toupper)`/`$(tolower)` +
  CaseConvert) — those are still live.
- Low priority: the struct is already `#[allow(dead_code)]`, so this is
  hygiene, not a correctness issue.
