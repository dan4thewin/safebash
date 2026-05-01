title: DD#42 — getenv and argv as builtins
status: designed

`$(getenv HOME)` replaces `$HOME`. `$(argv 1)` replaces `$1` at script level.
Tainted results. Return status handles missing values.
Codegen supports them but not exposed as standalone commands yet.
