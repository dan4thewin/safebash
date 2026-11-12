title: "janet: `read -a` produces one element too many"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "janet: `array a=(); echo \"p q\" | read -a a; echo $(len a)` prints `2` (native/c print 2; janet prints 3)"
  - "spec `taint.sh` tl.8 passes in janet mode (currently gated native,c)"

## Found

2026-09-08, adding the taint-laundering specs (journal 259). The clean-control
spec `tl.8` passes in native and c and fails in janet:

```sh
array a=(); echo "p q" | read -a a
echo "$(len a)"
```

native/c print `2`. janet prints `3`.

Likely the trailing newline is being counted as a third (empty) record — the
terminator-semantics elision that `sash_split_into_array` implements for the
native/c path (spec 22.27: interior empties are kept, the single trailing empty
from a final delimiter is dropped).

## Note

Independent of the taint fix it was found alongside — it is a plain
element-count discrepancy on a *clean* pipeline. Related but distinct from the
already-gated 22.26/22.27, which are skipped in janet for a different reason
("janet lacks custom-delimiter array reads"); this one uses the default
whitespace split.

## DONE 2026-09-12 — root cause was not the trailing newline

The filing's guess ("the trailing newline is being counted as a third empty
record") was wrong, and the gate as written is now unwritable: `read -a` is
SB5057-rejected under DD#99.

The real cause: `emit_janet.rs::detect_lastpipe` classified the lastpipe target
by `BuiltinCmd::from_name`, which maps **both** `read` and `readline` to
`BuiltinCmd::Read`. An array target was therefore bound as a **scalar string**,
and `len` was counting *bytes*, not elements — the same reason
`${a[0]}` returned `97` instead of `p`.

Fixed in `remove-retired-surface-implementations` by splitting janet's
`Lastpipe` into `Readline { name }` / `ReadArray { name }` keyed on the literal
command name, with NUL splitting and trailing-empty elision for the array form.

`spec/taint.sh` `tl.8` is **un-gated for janet** and passes; `spec/commands.sh`
`rt99.14` pins the element values directly.
