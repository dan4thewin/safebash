title: sash -c 'script' — compile and run inline source
status: ready
depends: []
severity: feature
gates:
  - tests
  - commit

Add `-c` flag to sash: `sash -c 'echo hello'` compiles and runs
inline source. Same as `echo '...' | sash -` but cleaner for
one-liners and spec tests.

Ref: spec test harness needs this for acceptance tests.
