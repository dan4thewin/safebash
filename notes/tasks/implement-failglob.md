title: Implement failglob for inline globs (DD#20)
status: ready
severity: correctness
depends: []
gates:
  - tests
  - commit

DD#20 says failglob is on for inline star globs in argument position.
If `cmd *.sh` matches nothing, it should be an error, not a silent
pass-through of the literal `*.sh` string.

Verify current `sash_glob` runtime behavior:
- Does it return an error / set exit status when no matches?
- Does codegen check the result and abort?

If not implemented, add runtime error on zero matches and codegen
to check the glob result count.
