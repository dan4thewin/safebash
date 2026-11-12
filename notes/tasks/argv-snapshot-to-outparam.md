title: "argv snapshot → out-param builtin: array a=$(argv N) becomes `argv [N] a`"
status: done
priority: P2
kind: refactor
depends: [DD60-argv-trio]
gates:
  - "TDD: spec tests below written first and red before code"
  - "`argv a` fills array a with all program args; `argv N a` fills the argv[N:] slice (array last)"
  - "`array a=$(argv)` / `array a=$(argv N)` (old snapshot assignment) is rejected"
  - "the ArgvSnapshot command-sub→array machinery is deleted (no command-sub yields an array)"
  - "splat `$(argv)`/`$(argv N)` (for-in) and scalars `$(arg N)`/`$(argc)` unchanged"
  - "taint/bless unchanged: `argv a` fills a tainted array, bless-all before reading"
  - "native spec clean (only cf.c7/cf.c8); rust suite no net regression; complexity gate green"

# argv snapshot → out-param builtin (DD#60 revision)

DD#60 #1 (2026-06-17): the snapshot moves from `array a=$(argv N)` — the
*only* place a command substitution yields an array — to the out-param
pattern used by `mapfile -m m` / `read -a a` / `match S P arr` (DD#63):
the array is the trailing positional. Deletes the `ArgvSnapshot`
command-sub→array special case.

Splat (`for x in $(argv)`) and scalars (`$(arg N)`, `$(argc)`) are **not**
touched (that was #2/#3, deferred).

## Spec tests to write first (migrate spec/argv.sh)

Pre-declare the array like `mapfile`/`read -a` (`array a=(); argv a`) —
unless we decide `argv` auto-declares (confirm against `mapfile`).

- **av.n1'** all args: `run_sash 'array a=(); argv a; bless x in a [[ -n $x ]]; for x in @a; do echo "$x"; done' a b c` → output ends `…\na\nb\nc` (argv[0] then a b c).
- **av.n2'** len of a slice snapshot: `array a=(); argv 1 a; echo $(len a)` (args `a b c`) → `3`.
- **av.n3'** two independent snapshots: `array a=(); argv 1 a; array b=(); argv b; echo $(len a) $(len b)` (args `a b`) → `2 3`.
- **av.n4'** join over a blessed snapshot: `array a=(); argv 1 a; bless x in a [[ -n $x ]]; echo $(join @a)` (args `a b c`) → `a b c`.
- **av.x5'** subscript after bless-all: `array a=(); argv a; bless x in a [[ -n $x ]]; echo $a[0]` (args `a b c`) → argv[0].
- **av.n6'/n7'** no-bless negatives still SB5020: `array a=(); argv 1 a; echo $(join @a)` and `… echo $a[0]` → reject.
- **rej1** old snapshot form rejected: `array a=$(argv)` → reject (SB5025 splat-in-non-splat, or a dedicated diagnostic directing to `argv a`).
- **rej2** `argv N a` slice past end → empty array (zero iters), not abort (parity with old `$(argv N)`).
- **splat-unchanged** `for x in $(argv 1); do bless x [[ . ]]; echo "got:$x"; done` (args `a b`) → `got:a\ngot:b` (av.x6 regression).

## Implementation outline (after specs are red)

1. **Parser**: `argv` parses as a command with an out-array arg and an
   optional leading integer (`argv [N] ARRAY`), not only as the
   `$(argv …)` substitution. (The substitution form stays for the splat.)
2. **SSA**: replace `SsaValue::ArgvSnapshot { start }` (a value) with an
   **instruction** that fills a named array — model on `SsaInst::Mapfile`
   / `ReadArray` (an out-param fill). Drop the `array a=$(argv)` snapshot
   detection in `transform/statements.rs::compute_declaration_value`.
3. **Codegen**: reuse the existing `sash_argv_snapshot(start)` extern, now
   driven by the fill instruction writing into the array's table (instead
   of an assignment-value lowering in `codegen/values.rs`).
4. **Validator**: `array a=$(argv …)` (assignment RHS) → reject. `$(argv)`
   in non-splat / non-assignment context already SB5025; keep. `argv [N] a`
   as a command → allowed; fills a tainted array (DD#61).
5. **Converter** (`sash-convert`): if it emits `array a=$(argv …)`,
   switch to `argv [N] a`.

## Resolved (DG, 2026-07-08)

- **`argv [N] a` AUTO-DECLARES `a`** — no prior `array a=()` needed,
  mirroring `mapfile -t a` / `read -a a` (verified: sash already
  auto-declares on assignment and on bulk-read; the `array a=()` before
  `mapfile` in the DD#60 specs is stylistic, not required — spec 24.4
  `mapfile -t arr` on an undeclared `arr` compiles past declaration and is
  rejected only by the taint check on use). Requiring an explicit decl
  would make `argv` the odd one out vs. read/mapfile. The auto-declared
  array is tainted (DD#61), so a use needs a bless-all.

## Cross-refs

- DD#60 "Revision 2026-06-17"; the out-param pattern is shared with DD#63
  (`match S P arr`). Same insight: array-*return* is the murky special
  case; out-params are the general pattern.
- Migrate `spec/argv.sh` av.n*/av.x5 and the codegen/e2e tests that use
  `array a=$(argv …)`.
