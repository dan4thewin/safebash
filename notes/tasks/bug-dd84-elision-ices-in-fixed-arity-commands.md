---
title: "DD#84 elision ICEs in eleven commands — patch the denylist, or invert it?"
status: blocked
priority: P1
kind: bug
depends: []
blocked_on: "DG — the fix approach is a DD#84 language-surface question, not just a patch"
gates:
  - "`${p:~}` in an operand of cd/match/sub/gsub/toupper/tolower/strlen/basename/dirname/return/exec is a diagnostic, never an ICE"
  - "echo / printf / external commands / user-function args / array-literal elements still elide (they are the genuinely variadic positions)"
  - "spec coverage for each newly-rejected command, in both modes"
  - "sweep zero flips against the baseline at a22e0f3 (6750 passing / 13 skipped / 6763 rows)"
---

# The bug

`${p:~}` (DD#84 elide) panics the compiler in eleven commands:

```
ICE: ElideEmpty (:~) must be expanded by a list builder,
     not lowered as a scalar value        (codegen/values.rs:116)
```

`cd`, `match`, `sub`, `gsub`, `toupper`, `tolower`, `strlen`, `basename`,
`dirname`, `return`, `exec`. Minimal case:

```sh
str p=""
str r=$(toupper ${p:~})
echo "$r"
```

Probe the rest with a script that **reads every variable it writes**, or
SB5050 fires first and masks the result (that is how an earlier pass of this
probe wrongly reported five of these as already-guarded).

# Root cause

`validate.rs:3706`:

```rust
let elidable = !matches!(expr_literal_str(&c.name), "readline" | "read");
```

Allow-by-default with two exceptions. Both exceptions were added *reactively*
after they ICEd — the comment above them says so: "(Before this it reached
codegen and panicked.)" Everything else is assumed variadic.

The rule itself is already written down correctly, twelve lines below, in the
message `check_elide_pexp` emits:

> Say VARIADIC, not "a command argument". **A fixed-arity builtin's argument
> is a command argument too** …

So the knowledge is present and the wiring is not — the same shape as the
SB5070 splat bug fixed in `a22e0f3` (journal 440), where the operand list
already named `match` but was consulted from only one of the two positions
`match` can occupy.

# The map (measured against `a22e0f3`)

| position | today | correct? |
|---|---|---|
| `echo`, `printf`, external, user-function args | elides | yes — real argv builders |
| array-literal element | elides | yes |
| `read`, `readline` | SB5046 | yes — patched reactively |
| `len`, `join`, `keys`, `unset`, `wait`, `close`, `signal`, `trap`, `matchall`, `attest`, `argv` | rejected by *other* codes (arity/type) | accidentally safe |
| the eleven above | **ICE** | **bug** |

# The decision

**A — extend the denylist at 3706.** Name all eleven. Small, reversible,
matches existing style. But this list has now failed twice, the failure mode
is a compiler panic, and a new builtin is elidable by default — so it will
drift again.

**B — invert to an allowlist.** Elidable only for genuinely variadic
positions: `echo`, `printf`, external commands, user-function calls (the pass
holds the `Program`, so the function names are collectible), plus the
array-literal case already handled separately. Fail-closed: a new builtin is
non-elidable until someone opts it in. **No working program breaks** — every
program B newly rejects ICEs today.

B's sharp edge is `exec`. It is genuinely variadic, so DD#84's own words
("an argument list") make `exec cmd ${p:~}` legal; it ICEs because `exec`'s
codegen lowers args as scalars instead of through the argv builder. B would
reject it, which is a real (if currently unusable) restriction on the
language.

**C — B, plus fix `exec`'s codegen** to use the list builder, so `exec` can
stay in the allowlist honestly. Most work; keeps DD#84 whole.

Recommendation: **B now, with a follow-up task for `exec`'s codegen** (which
promotes it into the allowlist and turns C into a later, separable change).
The argument for inverting rather than patching is that the cost of being
wrong changes direction: under A a missed command is an ICE, under B it is a
spurious rejection that the next spec sweep finds immediately.
