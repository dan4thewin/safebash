# 126 — language sweep (DD#65-68), part 1

Session 2026-07-04 (continued). DG: "do the language sweep now" +
"stop worrying so much." Worked the DD#65-68 cluster. Zero regressions
throughout; native gate clean.

## Shipped this turn (pushed)

1. **dd67 sugar** — verified already-shipped: `name()`/`function`/flat &
   nested backticks (SB5020)/`[`-literal (e9849ad/def5c40/c3875d1). The
   remaining §3 (sash-convert canonicalize) is coupled to dd68's brace
   forms; deferred there.

2. **dd68 brace control flow** (c98ed29) — `if/while/until/for { }` bodies
   alongside `then/do` sugar. Parser-only, identical IR (verified
   brace==keyword in all 3 backends). Command arg parsing now stops at a
   top-level `{`. `else if` = else + nested if. Brace-group condition /
   missing body → SB4027. spec/brace.sh (25) native-only (parser feature,
   like convert.sh); pre-existing c/janet cf gaps filed
   (`cf-backend-gaps-brace-surfaced`).

3. **dd66 dispatch form — additive** (5231dd1) —
   `[[ SUBJECT ==|=~ PATTERN { grp } … || { grp } ]]`. `==` exact, `=~`
   unanchored regex; first-match-wins; `|| { }` default; statement-only
   (SB4028). `ir::Case` gained an `op` field; `build_dispatch_chain`
   carries it (native+c via shared SSA); janet `emit_case` op-aware (`=~`
   → sash-regex matchq). `parse_regex_rhs` ends the regex at a
   whitespace-separated `{` (glued `{n,m}` still works). spec/dispatch.sh
   (22) green in all 3 backends.

## Key technical notes

- **dd66 dispatch detection**: parse the boolean test normally; a `{` after
  the first `SUBJECT ==|=~ PATTERN` reinterprets it as arm 1 of a dispatch.
- **regex `{` ambiguity**: counted-repetition `{n,m}` and the dispatch arm
  `{` are both LBrace tokens; disambiguated by whitespace gap (span.start >
  prev token end).
- **dispatch pattern resolution**: `dispatch_pattern_string` strips quotes
  (== compares the literal value); legacy `=glob` keeps `expr_to_string`.
- **quoted-regex-in-=~** (`[[ $x =~ '…' ]]`) is a pre-existing gap (boolean
  =~ rejects it too) — dispatch specs use unquoted grouping.

## Remaining (filed)

- **dd66-remove-case** (P2) — the destructive half: reject `case`/`;&`/`;;&`,
  delete `=glob`/`fnmatch_impl`, migrate case specs → dispatch, converter,
  docs. Separated because it is spec-migration-heavy and destructive.
- **dd65 var modifiers** (P2) — the atomic drop+rebuild (toupper/tolower
  builtins, basename/dirname sugar, `:-` full-word interpolation across 3
  backends). Not started.

## Followups filed this turn
- cf-backend-gaps-brace-surfaced, dd66-remove-case.
