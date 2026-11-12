title: "`${a[-1]}` is reported as a \"string key\", and advised to \"use an integer\""
status: open
priority: P2
kind: diagnostic-quality
depends: []
gates:
  - "a negative literal subscript names negative indexing as the reason, not string-vs-int"
  - "the `map` arm keeps accepting `-1` as an ordinary string key"

```sh
array a=(1 2 3); echo "${a[-1]}"
```

```
error [SB4030] string key on indexed array 'a' — an int-indexed array is
indexed by number (DD#80). Use an integer, `${a[$i]}`, or `$(( … ))`;
declare `map a` for string keys
```

`-1` is an integer. The message calls it a string key, then advises using an
integer, and finally suggests `${a[$i]}` — which, with `i=-1`, is **accepted**.

## Cause

`is_string_key` (`type_check.rs:1103`) classifies a literal as a string key
when it is not *all* ASCII digits:

```rust
ExprKind::Literal(s) => !s.is_empty() && !s.chars().all(|c| c.is_ascii_digit()),
```

A leading `-` fails that test, so `-1` falls into the string-key arm.
`is_integer_literal` (:1116) has the same blind spot in the opposite
direction.

## Behavior is fine — only the wording is wrong

Characterised, and nothing here is unsafe or incoherent:

| source | result |
|---|---|
| `echo "${a[-1]}"` | SB4030 (rejected) |
| `a[-1]=9` | SB4030 (rejected) |
| `int i=-1; echo "${a[$i]}"` | compiles, empty |
| `echo "${a[$(( 0 - 1 ))]}"` | compiles, empty |
| `int i=-1; a[$i]=9; echo "${a[$i]}"` | compiles, prints `9` |
| `int i=-1; a[$i]=9; echo "${a[0]} ${a[1]} ${a[2]}"` | `1 2 3` — unharmed |
| `map m=([k]=v); echo "${m[-1]}"` | compiles, empty (a valid string key) |

A negative index is a distinct key in the store, not an offset, so there is no
out-of-bounds write even at `-100000`. And literal-rejected /
dynamic-permitted is the **deliberate** pattern this codebase already uses —
`validate.rs:7465` says so for `$(arg -1)`:

> A negative *literal* index (`$(arg -1)`) is a typo guard caught here; a
> dynamic negative index stays a runtime concern.

So the typo guard is working. It just explains itself as a type error.

## Shape of a fix

Test the subscript for a negative integer literal *before* the map/array
split, and give it its own message naming DD#60 — "no negative indexing in
v1; `-1` does not count from the end" — with the `map` arm unchanged, since
`-1` really is an ordinary string key for a map.

`validate.rs:7483` already has `is_negative_int_literal` doing exactly this
classification for `$(arg -1)`. It is in a different pass, so the check would
be duplicated or hoisted — hoisting it is the consequential part and is DG's
call.

## Notes

Found while probing `validate.rs:7485` for branch coverage: that function's
branches are cold because no spec exercises `$(arg -1)` either. A spec for
SB5038 would flip them and is independent of this task.
