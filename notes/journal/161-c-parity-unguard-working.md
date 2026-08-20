# 161 — c parity (batch 4): un-guard already-working tests

Triaged the 43 native-guarded c tests. Twelve already pass in c and just
needed their guards widened `native` → `native,c` (0 code changes, 0
regressions, +12):

- **e.b70.tbare / e.b70.tdash** (errors.sh) — bare `[[ ]]` → SB4018, `- [[ ]]`
  → SB4019. Compile-time must-consume rejects; fire in every backend.
- **63.2e / 63.2f / 63.cap6 / 63.cap7** (regex.sh) — `match S P` /
  `match S P ARR` must-consume rejects (SB4018/SB4019). The stale comment
  claimed these transform diagnostics weren't surfaced on `--emit-c`; they
  are (transform runs before codegen), verified.
- **bng.1 / bng.2 / bng.3 / bng.6** (errors.sh) — `!` precedence
  (`(!X) && Y`), `! X && action`, `! X || action`, and the predicate-function
  escape hatch. Run correctly in c.
- **ud.16** (undeclared.sh) — `echo $(csx=1; echo $csx)` asserts only
  validation cleanliness (no SB5031), which holds in c.
- **e.a70.silent** (errors.sh) — `if /bin/false` → else branch, `-z $rerr`.

## Left native-only by design (not c gaps)

- **55.20-30, gs.6, 25.8, 24.20, e.a70.regcomp** — assert `$rerr` (runtime
  stderr), which the c harness discards (`./test_bin … 2>/dev/null`). Testing
  these in c needs a harness change to capture c runtime stderr into `$rerr`
  (+ backend-independent message assertions). Filed conceptually under
  `descriptive-abort` (deferred).
- **57.46a-c** — `sash --emit-source/--annotate/--nolint` CLI-flag rejects;
  mode-independent (don't use `run_sash`), kept native-only to avoid 3×
  redundancy like convert.sh.

## Still needing implementation

- **17.3 / 17.4 / mcs.3 / mcs.4** — function / multi-statement / pipeline
  stdout capture (`$(f)`, `$(a; b)`, `$(a | b)` into a var). c emits
  CaptureStart/CaptureEnd as `/* TODO inst */`, so the body runs inline and
  the capture is empty (verified: mcs.3 → `a\nb\n[]`). → fn-stdout-capture.
- **14.2c, 14.3a-d, 15.1, 15.2, 32.1, 32.2** — compound (group/for/if/fn)
  pipeline stages. → compound-pipeline.
- **38.5 / 55.19** — `$!` background pid (`&` → `__subshell`/`__store_bg_pid`,
  TODO in c). → bgpid.
- **ivc.8** — `local int array` function-local: the restore mistypes the
  int-collection as an int scalar (`sash_atoi`) instead of the table slot.

## Files

- `spec/errors.sh`, `spec/regex.sh`, `spec/undeclared.sh` — guard widenings.
