title: DD#20 — Shell Options Are Compile-Time Decisions
status: implemented

No `shopt` or `set` commands at runtime. Options are baked in:

- `lastpipe` — always on
- `set -e` / `set -u` / `pipefail` — semantics baked into compiler
- `extglob` — rejected (DD#39: star-only glob, no `@()`/`+()`/`!()`)
- `globstar` (`**`) — rejected (use `find` for recursive traversal)
- `nullglob` — moot (globs go through arrays, check `${#arr[@]}`)
- `failglob` — on for inline star globs in argument position
- `dotglob` — off
- `nocaseglob` — off
