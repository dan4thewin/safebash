# 197 — for-in-cmdsubst pipe-while-read coverage (+ c EOF bug)

Coverage-grinding `control_flow.rs::emit_cmdsubst_pipe_while_read` (37 lines) —
the DD#28/#40 transform of `for x in $(cmd)` into `cmd | while read x`.

## Reachability

The loop var is tainted (command output), so it must be blessed with a
predicate: `bless x [[ -n $x ]]` (bare `bless x` is SB4014 — the argv.sh idiom
is `bless VAR [[ … ]]`). An *external* command in the cmdsubst is CST-taint-
rejected before the transform, but an *internal* one (`echo`) reaches it:

```
for x in $(echo a b c); do bless x [[ -n $x ]]; echo "$x"; done  →  a b c
```

`read` is line-based, so `echo a b c` is one iteration (`x="a b c"`), not three
— a deliberate divergence from bash's word-split (DD#28/#40). spec coverage.sh
cov.61 (native-only) pins it, covering emit_cmdsubst_pipe_while_read +
build_cmdsubst_stage + build_pipeline_stages_from_cmdsubst.

## c-backend bug found (filed)

native and c diverge at EOF: c runs one extra empty iteration
(`bless: x failed the gate` on stderr) that native doesn't. stdout/rc still
match. Filed `c-pipe-while-read-eof-extra-iter` (P3); cov.61 is native-guarded
until it's fixed.
