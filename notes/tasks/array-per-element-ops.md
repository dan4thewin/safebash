title: Array per-element operations
status: ready
depends: []
gates:
  - tests
  - commit

`${arr[@]/pat/rep}`, `${arr[@]#prefix}` — codegen handles
`${var/pat/rep}` for scalars but doesn't loop over array elements.
13 Brush test cases depend on this.
