title: "bug: sash-convert conflates hashes and arrays"
status: done
absorbed-by: dd75-len-collection-only-strlen
priority: P2
kind: bug
depends: []
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


## Unblocked 2026-09-12

DD#102 gave `strlen` a value operand, so the spelling this needed now exists
and works in all three backends:

    echo "${#a}"     # bash: bytes of element 0  ->  $(strlen ${a[0]})
    echo "${#a[i]}"  # bash: bytes of element i  ->  $(strlen ${a[i]})
    echo "${#a[@]}"  # bash: element count       ->  $(len a)

No language-surface work remains; this is now a converter-only change.

## Done 2026-09-13

All four rows land, and `${#a}` / `${#a[@]}` no longer convert alike:

| bash | bash says | converts to | sash says |
|---|---:|---|---:|
| `${#a}` (array) | 1 | `$(strlen ${a[0]})` | 1 |
| `${#a[@]}` / `${#a[*]}` | 3 | `$(len a)` | 3 |
| `${#a[i]}` | 1 | `$(strlen ${a[1]})` | 1 |
| `${#s}` (scalar) | 5 | `$(strlen $s)` | 5 |

`length_expansion` in `sash-convert/src/convert.rs` replaces the
subscript-stripping block. The subscript answers every row but the bare one,
which depends on whether the name is a collection — so a pre-pass
(`CollectionPass`) collects every name bash declares as one: an array literal,
a subscripted assignment, `declare`/`local`/`readonly` with `-a`/`-A`, and
`mapfile`/`readarray`/`read -a` targets.

**A wrong guess on the bare form cannot pass silently.** `$a` on a collection
is SB5064 and `s[0]` on a scalar is SB5068, both naming the fix. A name
declared outside the file is simply absent and lands on the scalar reading —
the common case.

### The scalar row had rotted past the filing

The original said `${#s}` was "correct today only by coincidence". By
2026-09-13 it was not correct at all: DD#75 made `len` collection-only, so the
converter's `$(len s)` on a scalar had become **SB4030 — a program that does
not compile**. The converter was never updated when the compiler split.

### A spec was pinning the bug

`57.13c` asserted `${#arr}` → `$(len arr)` *and* `${#s}` → `$(len s)` — the
conflation, written down as the expectation. That is why it survived: the
suite was green. It has been rewritten to the four-row expectation, with the
history in the comment.

### Second defect, found on the way

The round-trip gate could not be demonstrated, because the converted program
did not compile for a reason unrelated to lengths: **the converter never
introduced the DD#97 `array`/`map` keyword**. Filed and fixed in the same
change — see `notes/tasks/bug-convert-no-collection-keyword.md`.

### Specs

`57.74`–`57.85` in `spec/convert.sh` (plan 88 → 100), plus the corrected
`57.13c`. Two are end-to-end round trips: convert → compile → run → compare
with bash. Fail-first on revert fails **exactly 10** — `57.13c`, `57.75`–`.78`,
`.80`, `.81`, `.83`, `.84`, `.85` — the predicted set exactly. `57.74`, `57.79`
and `57.82` pass either way by construction (the old code already emitted
`$(len a)` for `[@]`, already left `${##…}` alone, and never added a keyword
to claim wrongly).
