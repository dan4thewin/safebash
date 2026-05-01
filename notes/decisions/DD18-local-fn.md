title: DD#18 — Local Function Definitions for Mocking
status: designed

`local fn name() { ... }` — function definition scoped to enclosing
block. Replaces `eval`/`declare -f` for test mocking. Designed, not built.
