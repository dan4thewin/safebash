title: argv/arg/argc trio — existing test migration
status: done
priority: P0
kind: refactor
depends: [argv-trio-validator]
gates:
  - all "${arr[@]}" → @arr in spec/ and sash/tests/
  - all $(argv N) (where intent was single element) → $(arg N)
  - all $(argc) expectations updated if intent was user-arg count
  - all top-level $@/$N/$# in spec source migrated to argv builtin forms
  - 291 + new tests pass with zero regressions after T2+T3 land

# argv/arg/argc trio — existing test migration

Mechanical cleanup of the existing test corpus to match the post-DD22
+ post-DD60 surface. Lands together with T2 (validator) — without
this, T2's validator rejects existing test sources and shows as
spurious regressions.

## Inventory

Run these against the index DB to enumerate exactly:

```bash
DB=.indexnudge-rust.db

# Quoted array splats (need bare-form migration)
sqlite3 "$DB" "SELECT file, line_num, content FROM lines_fts
               WHERE lines_fts MATCH '\"\\\"\\${\"' AND content LIKE '%[@]%'
               ORDER BY file, line_num;"

# All $(argv N) callsites (need scalar→$(arg N) migration where N is literal)
sqlite3 "$DB" "SELECT file, line_num, content FROM lines
               WHERE content LIKE '%\$(argv %' OR content LIKE '%\$(argv)%'
               ORDER BY file, line_num;"

# All $(argc) callsites (need semantic check — bash $# or C argc?)
sqlite3 "$DB" "SELECT file, line_num, content FROM lines
               WHERE content LIKE '%\$(argc)%'
               ORDER BY file, line_num;"

# Top-level positionals in spec source (will start failing after T2)
sqlite3 "$DB" "SELECT file, line_num, content FROM lines
               WHERE file LIKE 'spec/%' AND
                     (content LIKE '%\$@%' OR content LIKE '%\$1%' OR
                      content LIKE '%\$#%' OR content LIKE '%\$0%')
               ORDER BY file, line_num;"
```

Initial spot-check (2026-06-13):

  - `spec/control_flow.sh:105` — `"${a[@]}"`
  - `spec/wordsplit.sh:32,44,138` — `"${arr[@]}"`, `"${a[@]}"`,
    `${arr[0]}` (last is scalar — keeps quotes/braces; not a splat)
  - `sash/tests/codegen_tests.rs:2325,2354` — `"${arr[@]}"`,
    `"${!m[@]}"` (keys form)
  - `sash/tests/e2e_tests.rs:745` — `"${arr[@]}"`

## Migration patterns

### Pattern 1: Quoted array splat → bare

  for x in "${arr[@]}"; do ...; done
  →
  for x in @arr; do ...; done

Applies to value iteration; `@arr` is the canonical spelling (DD#41).
Keys form `"${!m[@]}"` → `$(keys m)`
(sash-native key iteration per DD41) — already supported, but
verify the existing test uses the bash spelling and migrate to
the sash-native form if it does.

### Pattern 2: $(argv N) scalar → $(arg N)

  port=$(argv 1)
  →
  port=$(arg 1)

The DD42 example in `bless` rules (`port=$(argv 1)`) is now
`port=$(arg 1)`. Update DD24 example text (handled in T1) and any
DD42 references in `notes/`.

### Pattern 3: $(argc) bash-$# semantics → adjust

If existing code uses `$(argc)` and expects bash `$#` (user-arg
count), rewrite to `$(($(argc) - 1))` OR change the test to assert
C argc value. T1 doesn't pre-decide; per-callsite judgment.

### Pattern 4: Top-level $@/$N/$# → builtins

For test scripts that ran at top-level positionals (rare but
possible in the sash compiler invocation context):

  for x in "$@"; do ...; done           (top-level)
  → for x in $(argv 1); do ...; done

  echo "$1"  (top-level)
  → x=$(arg 1); bless x; echo "$x"

Inside `fn ... { ... }` bodies, no change.

## Sequencing

Recommend a single PR that lands T2 + T6 + the unwrap removal in T3
together. Splitting risks intermediate states where either the
validator is too strict or too loose for the test corpus.

If that PR is too large:

  1. Land T6 alone — migrate tests to the new surface (still compiles
     today because T2's stricter rules aren't in yet, and the new
     forms aren't yet rejected by validator)
  2. Land T2 + T3 unwrap removal — once tests are migrated, the new
     validator rules and unwrap removal land cleanly

## Risks

  - Tests that pin error message text via `errors.sh` may need text
    updates for the new SB-error redirect strings (T1's e.dd46.*
    captures the post-change text; existing pinned text needs
    updating if any).
  - Janet backend tests (`janet_tests.rs`) — verify janet behavior
    matches the new design or update tests to skip cases that don't
    apply.
  - C backend tests (`c_tests`) — sash differential oracle should
    catch divergences. After migration, both backends should match.

## References

  - [argv-trio-design](argv-trio-design.md)
  - [argv-trio-validator](argv-trio-validator.md)
  - [argv-trio-codegen](argv-trio-codegen.md)
