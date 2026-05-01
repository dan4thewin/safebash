title: DD#20 — Shell Options Are Compile-Time Decisions
status: implemented

No `shopt` or `set` commands at runtime. Defaults: `nullglob` always on,
`lastpipe` always on, `set -e`/`set -u`/`pipefail` semantics baked in.
`extglob` superseded by DD#39 (star-only glob).
