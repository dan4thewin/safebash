title: Compound command redirects
status: ready
depends: []
gates:
  - tests
  - commit

Pipeline redirects (`cmd | cmd > file`) and remaining compound
command redirect edge cases. `for…done > file`, `while…done | cmd`,
`if…fi > file` — codegen has partial support (DD#27 session added
basic compound redirects) but pipeline-level redirects are missing.
