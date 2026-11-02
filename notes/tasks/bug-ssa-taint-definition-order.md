---
title: "SSA taint classification depends on block order"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "The nested-branch reproducer is rejected by the SSA backstop under --no-taint-cst in native and C modes"
  - "Definition classification reaches a fixpoint rather than assuming block order is data-flow order"
  - "Existing clean phi and attest-retag cases remain accepted"
---

B94, rechecked during W3 (journal 562). On e5f4bcdb, both backends compile
and print `[dirty]` with `--no-taint-cst`:

```
x=safe
if [[ $(echo a) == a ]] {
    if [[ $(echo b) == b ]] { x=$(/bin/echo dirty) }
}
echo "[$x]"
```

`verify_ssa_taint` classifies definitions in one pass. A phi can be visited
before an input's classification, and the result is not revisited.
This was open on both frozen reference trees; it is not one of the loop
fixes being ported. W4's process partition is also a separate concern.
With the normal front-end check enabled, both the W3 baseline and final
compiler report SB5020 at the final read, in both backends.

The owned probe used the same program on one line. Results are
`followups-native.tsv` and `followups-c.tsv` in the session evidence
`files/port-w3-probes.tar.gz`. DD51 and the defense-in-depth deferral in
`ssa-taint-full-backstop` govern the eventual fix.
