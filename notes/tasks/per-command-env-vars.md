title: Per-command env vars for externals
status: ready
depends: []
gates:
  - tests
  - commit

`FOO=bar cmd` syntax not compiled. This is the documented
replacement for rejected `export` (SB5001) — without it that
rejection has no escape hatch.
