---
title: "DD#84 elision ICEs in eleven commands — patch the denylist, or invert it?"
status: done
priority: P1
kind: bug
depends: []
resolved: "2026-09-28 — DD#84 settled it; option B (allowlist) landed, plus the exec codegen fix option C called for"
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


---

# Resolution (2026-09-28)

**Not a language-surface question after all.** Re-reading DD#84 at DG's
prompting, the "Elidable positions" section is a *closed three-item list*:

> - an external / command argument list (`ls ${args[0]:~} rest`);
> - a `sash`-**varargs** builtin arg list (`echo ${x:~}`);
> - an array literal element (`array a=(x ${y:~} z)`).

The operative word is **varargs**, which already excludes every fixed-arity
builtin without any amendment. Option B — invert to an allowlist — is not a
narrowing of the language; it is the DD as written. The decision tree's step
1 applies (settled by a source of truth), so it was implemented rather than
asked about.

The discriminator is `BuiltinCmd::from_name`: a name it does not recognise
is an external program or a user function call, both of which are bullet
one. `command_args_elidable` (validate.rs) is now total over that enum with
no wildcard, so the totality gate forces a decision when a builtin is added.

**`exec` was option C, and got it.** Its arguments are an external command's
argument list — bullet one — so the DD grants it elision, and rejecting it
would have made the compiler non-conformant. The ICE was a codegen fault:
both backends had their own stale copy of the argv builder. Fixed first, in
its own commit, which also closed a silent `execve` truncation the same
duplication had been hiding (see journal 441).

Landed: the exec codegen fix, then the allowlist. Sweep 6809 rows, 6796
passing, 13 skipped, zero failing, **zero flips**. Specs 84.37-84.52 and
ex.16-ex.22.
