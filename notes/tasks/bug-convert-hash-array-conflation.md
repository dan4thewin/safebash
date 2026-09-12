title: bug: sash-convert conflates hashes and arrays
status: blocked
absorbed-by: dd75-len-collection-only-strlen
priority: P2
kind: bug
depends: [strlen-cell-operand-spelling]
gates:
  - "bash `${#a}` on an array converts to the length of element 0, not the element count"
  - "bash `${#a[@]}` / `${#a[*]}` convert to the element count"
  - "the two no longer convert to identical output"

# bug: sash-convert conflates `${#a}` with `${#a[@]}`

```sh
a=(x y z)
echo "${#a}"       # bash: 1   (length of ${a[0]}, i.e. "x")
echo "${#a[@]}"    # bash: 3   (element count)
```

`sash-convert` emits `echo "$(len a)"` for **both**, and `len a` is 3. So
`${#a}` silently converts to a program that prints `3` where bash prints `1`.

## Cause

`convert.rs:980-986` deliberately strips the subscript before emitting:

```rust
// Strip any subscript (`[@]`/`[*]`/`[i]`) — `len` dispatches
// on the whole collection.
let base = rest.split('[').next().unwrap_or(rest);
patches.push(Patch::replace_node(node, format!("$(len {})", base), None));
```

The comment is accurate about `len`'s polymorphism but wrong about bash's:
`${#name}` and `${#name[@]}` are different questions in bash, and stripping
the subscript erases the distinction. It also mis-handles `${#a[i]}` (length
of one element) the same way.

The scalar case `${#s}` is correct today only by coincidence — a scalar has no
subscript to strip, and `len` on a scalar happens to mean byte length.

## The fix falls out of the `len`/`strlen` split

`dd75-len-collection-only-strlen` makes the correct mapping mechanical, with
no type inference required, because bash already distinguishes the spellings:

| bash | meaning | emit |
|---|---|---|
| `${#s}` | byte length of a scalar | `$(strlen s)` |
| `${#a}` | byte length of element 0 | `$(strlen a[0])` |
| `${#a[@]}`, `${#a[*]}` | element count | `$(len a)` |
| `${#a[i]}` | byte length of element i | `$(strlen a[i])` |

Today only the third row is expressible, which is why the other three collapse
into it. **This bug should be fixed as part of that task**, not before it —
fixing it first would mean writing the mapping twice.

## Found

2026-09-12, backlog review, while checking whether the `len`/`strlen` split
would complicate `sash-convert`. It turns out to simplify it, and surfaced
this on the way.

## Disposition (DG, 2026-09-12)

Fixed **inside** `dd75-len-collection-only-strlen` rather than separately, so
the four-row mapping is written once. This filing stays as the record of the
defect and its reproduction; the gates above are the split task's to satisfy.

## Blocked on a spelling (2026-09-12)

Two of the four rows need the byte length of one array element, and that
question has no spelling yet: `$(strlen a[0])` does not parse as a cell, and
`$(strlen ${a[0])}` is rejected SB5062 along with every other value form.
Filed as `strlen-cell-operand-spelling` for DG. The other two rows
(`${#a[@]}` / `${#a[*]}` -> `$(len a)`) are unblocked but are not worth
landing alone -- the point of the four-row table is that it is written once.
