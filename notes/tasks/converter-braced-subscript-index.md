title: "sash-convert: map bash arithmetic subscripts to sash explicit braced-index forms (DD#80)"
status: done
priority: P3
kind: convert
depends: []
gates:
  - "bash `${a[i]}` (arithmetic subscript, bare var) converts to sash `${a[$i]}`"
  - "bash `${a[i+1]}` / `${a[expr]}` converts to `${a[$(( … ))]}`"
  - "bash `${a[a[0]]}` (nested) converts to `${a[${a[0]}]}`"
  - "no regression in convert.sh"

## Context

DD#80 tightened the braced subscript surface: `${a[K]}` takes a DIRECT key
(integer literal, `$var`, an expansion, a quoted string, or a clean-identifier
bareword — maps only). Bash instead evaluates every subscript arithmetically, so
bash idioms that sash-convert currently passes through verbatim now reject:

- `${a[i]}` (bare var) → sash **SB4030** (string key `i` on an int array).
- `${a[i+1]}`, `${a[a[0]]}` → sash **SB4041** (ambiguous/implicit index).

The converter has no subscript-specific handling today (it passes them through),
and `convert.sh` has no subscript coverage, so nothing regressed — but converted
scripts that use array subscripts will fail to compile until the converter emits
the explicit sash forms.

## Approach

In `sash-convert`, when rewriting a `${a[K]}` where `K` is a bash arithmetic
subscript:

- a lone identifier `i` → `${a[$i]}` (interpolate the var).
- an arithmetic expression `i+1` / `f(x)` → `${a[$(( … ))]}`.
- a nested subscript `a[0]` → `${a[${a[0]}]}` (braced) or `${a[$(( a[0] ))]}`.
- a numeric literal `5` → unchanged (`${a[5]}`).
- for a map, a bareword key stays a bareword; a non-identifier key gets quoted
  (`${m["content-type"]}`).

Add convert.sh coverage for each. Distinguish array vs map from the declaration
where possible; otherwise prefer the `$(( … ))` arithmetic form (safe for an
int-indexed array) and flag ambiguous cases for review (DD#66 converter policy).

## Provenance

DD#80 migration section. Filed during the DD#80 implementation (journal 171).

## Outcome (journal 293)

Done. `subscript_expansion` / `subscript_key` in `sash-convert/src/convert.rs`
classify the key and rewrite it; `CollectionPass` now also records which names
were declared `-A`, since a bareword key is already direct on a map and a
variable read on an array. Specs `57.86`–`57.92` in `spec/convert.sh`.

Three things the filing did not anticipate:

- **tree-sitter-bash 0.25 cannot parse `${a[a[0]]}` at all.** It yields an
  `ERROR` node spanning the whole expansion, not an `expansion` node, so the
  nested gate could not be met by matching on `expansion`. The text inside the
  ERROR span is still exact, so `walk_node` now has an `ERROR` arm that runs
  the same rewrite off the raw text and returns early (skipping the recursion
  that would otherwise patch the malformed children underneath).

- **`${a[@]}` was drawing a spurious `# MANUAL: ${var@X} parameter
  transformation` note** — pre-existing, and caused by testing
  `inner.contains('@')` when `@` in a subscript is a splat. Fixed with
  `is_param_transform`, which requires the `@` to be followed by exactly one
  letter and preceded by a name. Pinned by `57.92`.

- **The `${a[i+1]}` gate is met, but the output does not compile when `i` is a
  string** — `${a[$(( i+1 ))]}` is the correct sash spelling and fails only
  because bash's implicit string→int coercion has no converter policy yet.
  That is a general converter gap, not a subscript one: see
  `converter-string-var-arithmetic.md` (blocked on DG).
