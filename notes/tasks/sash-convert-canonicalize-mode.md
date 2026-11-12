title: "sash-convert canonicalize mode: rewrite all sugar -> native-only forms"
status: ready
priority: P3
kind: feature
depends: [dd68-brace-control-flow, DD67-native-syntax-sugar-policy]
gates:
  - "sash-convert mode that rewrites ALL accepted sugar to native-only: then…fi / do…done -> { }, name()/function name -> fn name, flat `cmd` -> $(cmd)"
  - "compiler default stays PRESERVE (accepts either form); only the converter canonicalizes"
  - "spec/convert.sh cases for each rewrite; converted output compiles + runs byte-identical to the sugar form"

# sash-convert canonicalize mode (DD#67 §3)

Split out of `dd67-bash-sugar-sweep` (2026-08-10). DD#67 §1/§2/§4B (the
compiler-side sugar re-adds) and §4 (keep/drop table) are done; this is the
remaining §3 — the converter's canonicalize / native-only mode.

It was blocked on dd68's brace forms (the `then…fi`/`do…done` -> `{ }`
rewrite needs `{ }` bodies to exist). **dd68-brace-control-flow is now
done**, so this is unblocked. It stays Tier-6 (convert is last in the DG
ordering).

## Scope

A `sash-convert` mode (flag TBD; the compiler has no `--emit-source` — it
was removed when the converter split out) that rewrites every accepted
sugar spelling to the single native-only canonical form:

- `then…fi` / `do…done` -> `{ }` (needs dd68 brace forms — now present)
- `name() { }` / `function name { }` -> `fn name { }`
- flat `` `cmd` `` -> `$(cmd)`

Compiler default = **preserve** the user's form (both spellings accepted);
only this converter mode canonicalizes.

## Notes

- Thin CST rewrites, mirroring the existing sash-convert patch model
  (see `convert.rs`: `convert_tilde_word`, `convert_simple_expansion`,
  the declaration/backtick converters).
- The compiler already accepts both the sugar and the native form (DD#67
  §1/§2, DD#68), so the canonical output is guaranteed to compile — the
  tests just assert the spelling changed and behaviour is preserved.

## Related

- DD#67 (native-syntax-sugar policy), DD#68 (brace control flow).
- `notes/tasks/dd67-bash-sugar-sweep.md` (parent, done),
  `notes/tasks/dd68-brace-control-flow.md` (done).
