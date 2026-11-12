title: "BUG: `readline` into an int crashes the compiler (native) or emits C that does not compile"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "every program under What that reads into a declared int is rejected with a diagnostic, in both backends; none reaches codegen"
  - "k3 and the migration under Fix keep printing what they print today"
  - "specs pin r1 and r3 in both backends, asserting the code (not a bare `-n $err`)"

## What

Every program reads the line `9` from stdin.

| Id | Program | Native | `--emit-c`, then gcc |
|---|---|---|---|
| r1 | `int n=0` / `readline n` / `echo "got $n"` | SB5020 (with `--no-taint`: the ICE below) | SB5020 |
| r2 | `int n=0` / `readline n` / `echo "got ${n:!}"` | ICE | gcc: "'n' undeclared" |
| r3 | `int n=0` / `readline n` / `attest n [[ -n "$n" ]]` / `echo "got $n"` | ICE | gcc: "'n' undeclared" |
| t3 | `int n=0` / `readline n` / `attest n [[ "$n" =~ ^[0-9]+$ ]]` / `echo "got $(( n + 1 ))"` | ICE | gcc: "'n' undeclared" |
| k2 | r3 without the final `echo` | ICE | gcc: "'n' undeclared" |
| k3 | `str s=""` / `readline s` / `attest s [[ -n "$s" ]]` / `echo "got $s"` | prints `got 9` | prints `got 9` |

The ICE is "panicked at sash/src/codegen/types.rs:34:32: ICE: expected string
var, got int" (`VarSlot::str_ptr`), and the compiler exits 1. `--emit-c`
exits 0 and writes C that gcc rejects. The results are the same on the base
of the port (1309385), on HEAD before D1 (bf48bec), and on the
`taint-walker-redesign` branch.

No pass before codegen rejects the read, so a string (what `readline`
produces) reaches an int's slot. D1 makes this easier to
hit: its SB4030 for `attest n [[ -n $n ]]` advises quoting, and the quoted
form is r3.

## Fix

Recommendation (made during D1, not yet ruled): make `readline` into a
declared int SB4030 in type_check.rs, as the scalar twin of
`collection_fill_error` (sash/src/type_check.rs:2348), which already rejects
a string-producing fill of an int collection. DD#86: a variable should never
change type. The migration reads into a string and converts after the attest.
Measured on D1 in both backends:

```sash
str s=""
readline s
attest s [[ $s =~ ^[0-9]+$ ]]
int n=$(( ${s:?not a number} ))
echo "got $(( n + 1 ))"
```

It prints `got 10` for `9`. For `x` it prints "attest: s = 'x' failed the
gate" and exits 1.

## Related

- `bug-autodeclared-collection-scalar-read-leaks-pointer` (done): the same
  family for collections; its control covered a str target only.
- `q-attest-shorthand-int-operand`: this is the one measured channel that
  taints an int.

Found during the batch-531 port (step D1, following its attest advice into a
tainted int). Filed under DG's rule for the port that questions and
uncovered cases become backlog tasks.
