# Brush Suite Triage — 2026-04-25

Ran against sash built from commit after DD#51 + codegen panic cleanup.

## Summary

| Status | Count | Previous (2026-04-22) |
|--------|-------|-----------------------|
| PASS | 160 | 173 |
| FAIL_OUTPUT | 59 | 78 |
| FAIL_COMPILE | 75 | 43 |
| NA | 300 | 300 |
| SKIP | 27 | — |
| TIMEOUT | 3 | — |
| **Total** | **622** | — |

PASS went down because the taint gate now catches more (54 SB4020
rejections vs 0 before). FAIL_OUTPUT improved from 78 → 59.

## Compile Failures by Category (75 total)

| Code | Count | Notes |
|------|-------|-------|
| SB4020 taint | 54 | Expected — scripts use tainted data without bless |
| ICE/panic | 6 | Real bugs — see below |
| SB4003 indirect | 5 | Expected — `${!var}` rejected |
| SB4004 dynamic | 3 | Expected — variable as command name |
| SB5001 export | 3 | Expected — `export` rejected (per-cmd env not yet built) |
| BACKTRACE | 2 | Panics without ICE prefix — need investigation |
| SB5004 procsub | 1 | Expected — process substitution rejected |
| SB5002 heredoc | 1 | Expected — heredoc cmdsub rejected |

### ICE/Panic details (6+2 = 8 total)

These are real compiler bugs that need fixing:

- `arithmetic.yaml::Nested expressions` — `$(($(echo -n 1; echo 2) + 37))`
  cmdsub inside arithmetic
- `arrays.yaml::Fill associative array` — complex function with shift+loop
- `basic.yaml::Ensure ~ is resolvable` — tilde in `test ~`
- 3 more (unnamed in triage)
- 2 BACKTRACE crashes (word_expansion substring/transformation)

## Output Failures by Category (63 total)

| Category | Count | Description |
|----------|-------|-------------|
| wrong-output | 34 | Sash produces different output than bash |
| partial-match | 16 | Some lines match, some don't |
| empty-output | 8 | Sash produces nothing, bash has output |
| extra-output | 5 | Sash produces output, bash doesn't |

### Top wrong-output bugs (actionable)

**Arithmetic side effects (3 cases):**
`(( x = 1 ))` doesn't persist the assignment. `x += 1` in arithmetic
doesn't update the variable. This is a known limitation — DD#31 says
mutation and value are disjoint, but bash programs expect side effects.

**Parameter transformations (7 cases):**
`${var^^}`, `${var,,}`, `${var@Q}`, `${var@K}`, `${var@k}`, `${var@E}`,
`${var@a}` — case transformation and quoting transformations are not
implemented or produce wrong results.

**Substring replacement on arrays (5 cases):**
`${arr/pat/rep}`, `${arr[@]/pat/rep}`, `${arr/#pat/rep}`, `${arr/%pat/rep}`,
`${arr//pat/rep}` — per-element replacement on arrays not implemented.

**`${var}` when var is array (2 cases):**
`${var}` where var is an indexed array should expand to `${var[0]}`.
Sash returns empty.

**Compound command redirects (3 cases):**
`for...done > file`, `(cmd) > file`, function def with redirect —
redirect applies to compound command, not individual commands.

**Continue/break with N (2 cases):**
`continue N` and `break N` in nested loops — N > 1 not implemented.

**-v test (1 case):**
`[[ -v var ]]` should distinguish "set to empty" from "not set".
Current implementation treats as -n (non-empty), which is wrong.

**Multi-byte substring (1 case):**
`${var:0:1}` on multi-byte UTF-8 — byte vs character indexing.

**IFS joining (1 case):**
`"${arr[*]}"` should join with first char of IFS, not space.

### Correctly rejected (not bugs)

54 SB4020 taint, 5 SB4003, 3 SB4004, 3 SB5001, 1 SB5004, 1 SB5002 —
all are scripts that use features sash intentionally restricts. These
are working as designed.

## Recommended Fix Order

1. **Parameter transformations** — 7 cases, mostly mechanical
2. **Compound command redirects** — 3 cases, already on TODO
3. **Arithmetic side effects** — 3 cases, known limitation
4. **Array substring replacement** — 5 cases, on "out of scope" list
5. **Continue/break N** — 2 cases, small fix
6. **-v test fix** — 1 case, needs "set" vs "non-empty" distinction
7. **ICE/panic investigation** — 8 cases, compiler bugs
