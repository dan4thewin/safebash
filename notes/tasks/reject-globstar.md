title: Reject globstar ** pattern (DD#20)
status: ready
severity: correctness
depends: []
gates: [20.5]
  - tests
  - commit

DD#20 says globstar (`**` recursive glob) is rejected — use `find`
for recursive traversal. No validator rule catches `**` in glob
position. Add a validator rule that rejects it.
