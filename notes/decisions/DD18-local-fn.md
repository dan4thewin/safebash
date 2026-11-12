title: DD#18 — Local Function Definitions for Mocking
status: implemented

1. No current requirements.

---

~~`local fn name() { ... }` — function definition scoped to enclosing
block. Replaces `eval`/`declare -f` for test mocking. Designed, not built.~~

DD86 abandons this proposal and requires main-list function definitions.
