title: Reject unsupported multi-star glob patterns
status: open
priority: P2
kind: bug
depends: []
gates:
  - "known multi-star glob patterns such as a*b*c and ** are rejected in explicit calls and every glob sugar position, not silently matched or normalized"
  - "computed glob patterns enforce the same limit at runtime, reporting failure without changing the destination"
  - "valid single-star patterns and zero-star explicit patterns remain legal; regex patterns and ordinary literal text do not acquire a glob star limit"
  - "native/C CLI specs pin refusal and preservation with before/reversion controls and fresh ids for changed contracts"
  - "the `sash_glob` doc comment lists what the matcher supports, accurately"

## Selected remedy (DG, 2026-11-09)

DD39 now limits a glob to **at most one star**, and DD131 provisionally
packages the command/sugar enforcement in a separate-branch experiment.
This task owns rejection, not a general multi-star matcher.
`q-glob-semantics` coordinates that trial with literal directory support,
dynamic string patterns and recoverable errors. Implementation remains
open; the policy recording did not change the compiler.

## Historical F13 observation

In a directory holding `abc`, `axbxc`, `ab.c` and `a*c`:

```sh
for f in a*b*c { echo "$f" }
```

This prints nothing, in both backends; it compiles cleanly and exits 0.
Bash prints `ab.c`, `abc` and `axbxc`.

## Cause

`sash_glob` (`sash-rt/src/lib.rs`) splits the pattern at the **first** `*`
and matches `starts_with(prefix) && ends_with(suffix)`. For `a*b*c` the
suffix is `b*c`, compared byte for byte, so only a name literally ending in
`b*c` (a file named `axb*c`) matches. Its doc comment lists `"*.ext"`,
`"prefix*"`, `"prefix*suffix"` and `"*"`, the one-star shapes. Nothing at
compile time rejects a second star.

At the time, DD39 read "Star-only glob for files (`*` only, no `?` or
`[abc]`)" without a star-count limit. That is why this was filed as a
matcher bug. DG's 2026-11-09 choice supersedes the old expectation that
the example should match like Bash; the required outcome is now refusal.
The historical probe was not rerun for the policy recording.

## Fix

Use one pattern boundary shared by static validation and dynamic
operation handling. Reject a second star before enumeration; do not
silently collapse it or report no matches. Preserve the literal-prefix
repair in `bug-glob-directory-portion`. The two share classification
and runtime work and may land together; no new diagnostic or ABI was
chosen by the provisional decision.

Found during the batch-531 port (F13 glob probes, `m1`), unrelated to the
port itself.
