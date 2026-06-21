title: DD#20 — Shell Options Are Compile-Time Decisions
status: implemented

No `shopt` or `set` commands at runtime. Options are baked in:

- `lastpipe` — always on
- `set -e` / `set -u` / `pipefail` — semantics baked into compiler
- `extglob` — rejected (DD#39: star-only glob, no `@()`/`+()`/`!()`)
- `globstar` (`**`) — rejected (use `find` for recursive traversal)
- `nullglob` — moot: globs are arrays (DD#69). A `for` loop over a glob
  skips its body on zero matches; an external-command splat states its
  empty behavior via `@{a:-…}` / `@{a:?…}`.
- `failglob` — superseded by compile-time checks (DD#69): inline `*` in
  command-argument position is a compile error (globs go through arrays),
  and `@{a:?word}` is the explicit abort-on-empty splat.
- `dotglob` — off
- `nocaseglob` — off

## Clarification (2026-06-20): globs are array sugar

The glob lines above are reframed by **DD#69**: a glob (`*`, star-only
per DD#39) is *sugar for an array* — one verbatim element per directory
entry, no word splitting (DD#22). `*` is legal only in array-literal and
for-in positions; inline `*` as a command argument is rejected. Glob
results are clean (a dirent is trusted like `$(getppid)` — no taint).
Feeding an array to an external command requires an empty-disposition
(`@{a:-foo}` / `@{a:?bang}`); see DD#69.

