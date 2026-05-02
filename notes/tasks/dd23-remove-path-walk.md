title: Remove PATH walk from sash_exec
status: ready
severity: security
depends: []
gates:
  - tests
  - commit

DD#23 says sash never walks PATH. But `sash_exec` in `sash-rt/src/lib.rs`
(~line 184) calls `find_path(envp)` and iterates PATH entries when the
command has no slash.

Fix: if the command has no slash, that's a bug (compiler should have
resolved it). The runtime should write a diagnostic to stderr and
exit 127, not search PATH.

Note: this will break any tests that pass bare command names to
sash_exec. Those tests need to be updated to use absolute paths,
which means the compiler path resolution pass must land first (or
tests must hardcode paths temporarily).
