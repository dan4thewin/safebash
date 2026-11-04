---
title: "A counted jump inside a capture can escape into its caller's loop"
status: open
priority: P1
kind: bug
depends: []
gates:
  - "The reproducer prints clean hi in native and C modes, including with --no-taint"
  - "Break and continue counts clamp within the current body, never into its caller"
  - "Nested captures, named calls and self-tail calls retain their own loop and return targets"
  - "Leaving a capture always reaches its proper cleanup; an enclosing loop resumes normally"
---

Found while writing W3's body-boundary control (journal 562). This program
compiles with no diagnostics, exits zero and prints **nothing**, in both
backends on e5f4bcdb and on the initial W3 implementation, before its
zero-count guard:

```
str x=clean
for k in a b { : "$k"
    y=$(
        for j in c d {
            if [[ $j == c ]] { x=$(/bin/echo dirty); break 2 }
        }
        echo hi
    )
    x=clean
}
echo "$x $y"
```

DD86 makes the capture a fresh function body. Its one enclosing loop is
the `j` loop, so the count must clamp there and the result must be
`clean hi`. With a plain `break`, the same program does print `clean hi`;
`adjpolicy.8` preserves that ordinary nested-loop case. It replaces
`loopflow.15` with a legal adjacent attest before the later clean overwrite
(DD128, journal 568); it does not claim to repair counted jumps.

`transform/stmt_handlers.rs::capture_body_into` saves the return target,
reader, self-tail and condition contexts, but not `state.loop_stack`.
`loop_target` therefore sees both loops and selects the caller's exit,
bypassing the capture return/cleanup. Its count decoder is correct for
the stack it receives.

Also inspect `functions.rs::lower_inlined_body`, which likewise does not
save/clear that stack. A named-function runtime reproducer has not been
claimed here. Preserve self-tail lowering while establishing the body
boundary; do not simply reject a count greater than the local depth.

This is a pre-existing lowering defect, not the validator's loop-state
transfer. The affected body-entry functions are untouched by W3, so it is
filed separately under DG's bounded-port rule.

Evidence: `capture-counted-jump.sash`, the original failed draft control's
xtrace and `backlog-*.tsv` in session `files/port-w3-probes.tar.gz`.
