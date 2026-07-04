---
title: "DD#66 remove case/esac + =glob/fnmatch/;&/;;& (dispatch replaces it)"
status: ready
priority: P2
kind: feature
depends: [dd66-cond-form-dispatch]
gates:
  - "`case $x in a) echo a ;; esac` is a reserved-word compile error pointing at the [[ ]] dispatch form"
  - "`;&` / `;;&` no longer lex/parse (Tok::SemiAmp/DblSemiAmp removed)"
  - "=glob op + emit_glob + fnmatch_impl removed; sash_glob (FILENAME globs, e.g. for-in `*`) untouched"
  - "CaseItem.fallthrough/resume removed"
  - "existing case specs (control_flow cf.c3-c9, misc) migrated to the [[ ]] dispatch form"
  - "sash-convert rewrites bash `case … esac` -> [[ ]] dispatch (exact -> ==; a|b|c/glob -> =~ regex; *) -> || { })"
  - "docs GUIDE.md/SUBSET.md replace case with the dispatch form; native+c+janet clean; 0 regressions"
---

# Remove `case` — the DD#66 dispatch form replaces it

The additive dispatch form landed in 5231dd1 (`bug-janet-case-multipattern`
and the janet case work are subsumed once this lands). This is the
destructive half: delete the legacy `case…esac` construct now that
`[[ SUBJECT ==|=~ … ]]` dispatch covers it.

## Steps

1. **Parser**: make `Tok::Case` a reserved-word error (new/SB code) instead
   of `parse_case`; remove `parse_case`/`parse_case_item`. Remove
   `Tok::SemiAmp`/`Tok::DblSemiAmp` from the lexer + all matches.
2. **IR**: drop `CaseItem.fallthrough` / `CaseItem.resume` (only `case`
   used them; dispatch sets them false). `ir::Case` keeps `op` (now only
   `==`/`=~`).
3. **Lowering/backends**: remove the `=glob` arm from `glob_test` +
   `emit_glob` (native) / the `=glob` handling in emit_c and emit_janet
   `emit_case` (the `else`/`pats.len()==1` legacy-glob branch). Keep
   `sash_glob` (filename globbing) and `sash_fnmatch` if still used by
   filename globs — only remove the string-match `fnmatch_impl` case path.
   VERIFY for-in filename glob (`for f in *`) still works.
4. **Specs**: rewrite control_flow.sh cf.c3-c9 and any misc.sh `case` tests
   as `[[ ]]` dispatch. Add F1 (case rejects) / F2 (;&/;;& reject) specs.
5. **Converter**: `sash-convert` bash `case` -> dispatch.
6. **Docs**: GUIDE.md / SUBSET.md.

## Watch out
- `sash_glob` (filenames) must stay — only the case string-match glob goes.
- emit_janet `emit_case` currently also has the `bug-janet-case-glob` gap;
  removing the legacy-glob path closes that (the dispatch `==`/`=~` paths
  are correct).
