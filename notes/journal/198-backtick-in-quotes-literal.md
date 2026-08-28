# 198 — backticks in quotes are literal (DG decision) + spec

DG: "backticks are only allowed as top-level sugar for `$( )`, and should not
[be] special within quotes." Confirmed the double-quote case = **literal**.

## The mismatch

`lexer.rs::scan_dq_backtick`'s doc claimed it "captures the backtick span as a
CommandSub so the validator can reject it (SB5020)" — but the actual behavior
was silent literalization (`"x` + `` `/bin/echo BOOM` `` + `y"` → the literal
string, command never run), and no reject fired. So the code intended to reject
but the reject path was gone, leaving a `CommandSub` StringPart that happened to
render as literal text — a fragile representation for something that must never
be executable in quotes.

## Fix (DG chose "literal")

`scan_dq_backtick` now emits a plain `StringPart::Literal` for the `` `…` ``
span (not a `CommandSub`), making "not special in quotes = literal" explicit and
removing any executable representation. Output is unchanged (still the literal
backticks); the doc is corrected.

## Spec

coverage.sh cov.62 (all backends): `echo "x` + `` `/bin/echo BOOM` `` + `y"`
→ the literal string, command not run. Covers scan_dq_backtick. 0 regressions
native/c/janet.
