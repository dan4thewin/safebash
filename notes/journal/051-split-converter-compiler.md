# Split: sash-convert + sash

**Date:** 2026-05-03
**Branch:** split-converter

## Motivation

sash has one parser (a tree-sitter-bash fork) serving two languages:
bash (input) and sash-native (compiler's language). Every sash-native
feature (`int`, `const`, `fn`, `bless`, `${var:s/re/str/}`) is a patch
to a grammar designed for bash. Syntax compromises accumulate.

Splitting into two programs acknowledges what already happened: there
are two languages. The converter bridges them.

## Design

### sash-convert (new crate)

Migration tool. Bash source → sash-native source.

- **Parser:** unmodified tree-sitter-bash (upstream, no fork)
- **Shellcheck as analysis:** runs shellcheck internally, consumes its
  JSON output as signal. Not a precondition — the user never runs
  shellcheck themselves. Unambiguous warnings (SC2006 backtick,
  SC2292 `[ ]`) → convert silently. Ambiguous warnings (SC2086
  unquoted variable — might be intentional word splitting) → emit
  `# CHECK:` hint for human review. Shellcheck tells the converter
  what the user *might* have intended; the converter decides what's
  mechanical vs. what needs human judgment.
- **Output:** `.sash` file (plain text) with:
  - Bash that's already valid sash-native: passed through
  - Convertible constructs: converted (e.g., `declare -i` → `int`,
    backtick → `$(...)`, `[ ]` → `[[ ]]`)
  - Known externals: `bless` lines injected (best-effort, from
    known_externals list, e.g., `date`, `hostname`)
  - Recognized but unknown externals: comment hints that bless is needed
  - Unconvertible constructs: marked with `# MANUAL:` comments
- **Flags:** `--annotate` (add `# was:` comments), stdin/stdout support
- **No `--no-lint`:** shellcheck is a precondition, not our job

### sash (compiler)

Compiles sash-native source → static binary (or Janet, SSA dump).

- **Parser:** modified tree-sitter-bash fork (existing, unchanged for now;
  future: purpose-built sash-native parser)
- **Input:** sash-native source only. Will accept trivial bash that's
  incidentally indistinguishable from sash code, but not bash in general.
- **Drops:** validator rules, `--emit-source`, `--annotate`, `--no-lint`,
  conversion logic from lower.rs
- **Keeps:** lower (rewritten for sash-native CST), IR, type_check, taint,
  transform, SSA, codegen, janet backend, `--extract`
- **Taint analysis:** rigorous, based on explicit `bless` in source.
  No known_externals list — converter already injected the bless lines.

### No shared crates

The two tools share no code. `known_externals` lives only in sash-convert.
Each tool defines its own diagnostic types.

### .sash file format

Plain text. Sash-native syntax. No metadata, no headers. The contract
between the two tools is the sash-native language itself.

## Task breakdown

### Phase 1: Create sash-convert crate
1. `cargo init sash-convert` in workspace
2. Add tree-sitter-bash dependency (upstream crate, not fork)
3. Port `known_externals.rs` into sash-convert
4. Build conversion engine: tree-sitter CST walk → source patches
   - Extract conversion logic from current `lower.rs`
   - Output via coercion-patch approach (like current emit_source.rs)
5. Add `# MANUAL:` annotation for unconvertible constructs
6. Add best-effort bless injection (known externals → real bless lines,
   recognized externals → hint comments)
7. CLI: file arg, `-o` output, stdin/stdout support, `--annotate`

### Phase 2: Strip sash of bash concerns
8. Remove validator, rules, known_externals from sash
9. Remove `--emit-source`, `--annotate`, `--no-lint` flags
10. Remove conversion logic from `lower.rs`
11. Tighten parser expectations: sash-native input only

### Phase 3: Tests
12. Converter tests: bash → sash-native golden files
13. Pipeline tests: sash-convert | sash → binary, compare output to bash
14. Existing compiler tests: adapt to use sash-native input directly
15. Ensure `mk test` runs both test suites

### Phase 4: Docs and cleanup
16. Update README, GUIDE.md, SUBSET.md
17. Journal entry with outcome assessment
18. If successful: write DD for the split

## Open — future work (not this experiment)

- Replace modified tree-sitter fork in sash with purpose-built parser
- Drop tree-sitter-bash fork entirely (sash-convert uses upstream)
- Interactive / bulk migration mode for sash-convert
