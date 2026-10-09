---
title: "`--target-user` — check command paths against a specific user's access, not just any execute bit"
status: next
priority: P3
kind: feature
depends: []
gates:
  - "a mode-0700 root-owned program is rejected for a non-root target user"
  - "the coarse behaviour is unchanged when the flag is absent"
---

## What

SB5095 (DD#23 addendum, 2026-10-09) rejects an absolute command head that
the resolution root cannot run. The executability test is deliberately
coarse:

```rust
self.kind == FileKind::Regular && self.mode & 0o111 != 0
```

— *some* user can execute this. DG, approving the rule: *"(maybe add a
`--target-user` switch to do fine-grained access checking)"*.

## Why it is a separate question

The compiler cannot know who the compiled binary will run as. A mode-0700
program owned by `root` is executable — by root — and rejecting it because
the *build* user cannot run it would be a false reject, which is the one
failure SB5095 cannot afford. So the coarse test is the correct default,
and anything finer needs to be told the answer.

`--target-user=NAME[:GROUP]` (or a uid:gid pair, which is the form that
works against a `--sysroot` whose `/etc/passwd` is not the build host's)
would let the compiler apply the real rule: owner bits if uid matches,
group bits if gid matches or the user is in the group, other bits
otherwise.

## Open questions for DG

- **Name resolution.** With `--sysroot`, should `NAME` be looked up in the
  *target's* `/etc/passwd`, the build host's, or should the flag simply
  require numeric ids? Numeric is unambiguous and needs no parsing; a name
  is what a person actually knows.
- **Scope.** Does this govern only command paths (SB5095), or also
  redirect targets and `[[ -r ]]`-style tests? The latter are runtime
  questions about runtime paths, so probably not — but it is worth saying
  so rather than leaving it implied.
- **Root.** uid 0 bypasses the execute-bit check in the kernel only if
  *some* execute bit is set. Worth encoding, since `--target-user=0` is
  likely the common case for an appliance image.

## Where

- `sash/src/lib.rs` — `PathFacts::is_executable` is the single predicate;
  it would need the target identity, and `PathFacts` would need `uid`/`gid`
  (neither is recorded today, and the squashfs listing reports them as
  `owner/group` *names*, which is its own lookup problem).
- `sash/src/resolve.rs` — `command_path_fault` phrases the diagnostic and
  would need to name the user it judged against.
- CLI plumbing alongside `--sysroot` in `cli_main`.
