title: Compiler path resolution pass (bare name → absolute path)
status: done
severity: feature
depends: []
gates: [23.1, 23.2]
  - tests
  - commit

DD#23 requires the compiler to resolve bare command names to absolute
paths at compile time. New pass after lowering (or during lowering):

- Walk all Command IR nodes
- For each bare command name (no slash), search the build host PATH
  or `--sysroot` for the binary
- Replace with absolute path
- Fatal compile error if unresolved

Flags:
- `--sysroot=/path` — resolve against a target root filesystem
- Default: resolve against the build host

Also emit a dependency manifest listing all resolved external binaries.

This blocks `dd23-remove-path-walk` in practice — tests will break
if the compiler doesn't provide absolute paths before the runtime
stops searching.
