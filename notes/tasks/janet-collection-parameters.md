---
title: "janet backend has no DD#100 collection parameters"
status: open
priority: P2
kind: feature
depends: []
gates:
  - "`show \"nums\" =nums` binds slot A in janet, as in native/c"
  - "spec/collparam.sh runs unguarded in all three modes (drop the 17 `in_modes native,c` guards)"
  - "slot writes are visible to the caller in janet (100.8, 100.9)"
---

## Context

DD#100 binds a caller's collection to a slot (`A`..`E`) **in the transform**
(`setup_callee_slots` / `resolve_args_name`). The janet backend renders from the
AST and never runs that stage, so it emits the argument as a literal:

```
$ sash --janet j.sh
(defn show [& _args]
  (print (string (or (get _args 0) "") " has " (string (length A)) " …")))
(var nums @["3" "1" "4"])
(show "nums" (string "=" "nums"))
```

`A` is an undefined janet symbol, and `(len A)` / `(join A ,)` degrade to
spawning `join` as an *external command*. So the feature is simply absent.

The **compile-time** half is fine: SB5065, SB5066 and the kind/taint rules are
all front-end, so all 17 diagnostic tests in `spec/collparam.sh` pass in janet
unguarded. Only the 17 runtime tests are guarded `in_modes native,c`.

## Why it is separate work

This is the same class as `janet-join-intrinsic-gaps` and the reason the old
`scope.sh` 86.23 was guarded: janet does not see transform-stage rewrites, so
every feature implemented there has to be re-implemented against the AST. Doing
that for DD#100 means teaching `emit_janet` the slot-binding calling convention,
which is a backend design question, not a port of the existing code.

## Related

- `notes/tasks/janet-join-intrinsic-gaps.md`
- `notes/tasks/janet-map-from-array.md`
- `notes/decisions/DD100-collection-parameters.md`
