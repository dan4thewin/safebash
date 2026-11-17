title: "Map literals silently drop unrecognized keyed or positional elements"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "No parsed element disappears when a literal contains a recognized map pair."
  - "Otherwise legal quoted keys survive; malformed mixed literals receive a located diagnostic."
  - "Real CLI specs cover both orders, all-quoted keys and retained ordinary map keys, with before/reversion controls."

# Map literals silently drop unrecognized elements

Found during the DD132 predicate work on 2026-11-17. This predates that
work: the frozen compiler at `3ddaab55` compiles the following and prints
`1:b`, silently discarding the second entry:

```sash
map m=([a]=b ["("]=good)
echo "$(len m):$(join m ,)"
```

The current native and C outputs for the corresponding caught-attest
fixture contain only the first entry. Its successful predicate is therefore
not evidence that the second key was checked.

`parse_array_literal` in `sash/src/parse/expr.rs` collects `elems` and
`assoc_pairs` separately. Once `try_parse_assoc_element` recognizes any
pair, the result contains only `assoc_pairs`; every ordinary parsed
element is discarded. The recognizer requires a single `Word` containing
the whole bracketed key followed by `=`, so a quoted component can take
the discarded route.

Do not silently discard input regardless of the eventual accepted key
grammar. Coordinate with
[the literal-brace key limitation](map-literal-key-cannot-hold-a-brace.md)
and [collection operand lexing](bug-array-literal-elements-not-operand-words.md).
Those records do not currently cover this silent-loss path. The predicate
implementation does not modify these parser functions.
