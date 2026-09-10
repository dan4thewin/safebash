# 224 — bless-fallback via `||` (bless in a caught `&&`/`||` position)

DG wanted the "use DEFAULT when a bless fails" idiom (the old journal-036
`${port:~8080}` sketch, never built). We considered reviving it as a `:!`
operator, but DG asked: *does `||` already solve it?* — `bless v [[t]] || v=dflt`.

## The finding

It should, but it mis-compiled. `bless v [[t]] || v=dflt`:
- invalid `v` → **aborted** (the `||` never caught the bless failure);
- valid `v` → ran `v=dflt` **anyway** (the branch read a stale status).

Root cause: a `bless` command has two lowerings — `if bless` goes through
`lower_if_bless` (branch on the test, retag clean on pass), but a bless as the
left operand of `&&`/`||` went through `lower_one` → the **abort** form
(`SsaInst::Bless`). So `||` saw an abort, not a catchable status.

The semantically-identical `if bless v [[t]]; then :; else v=dflt; fi` already
worked perfectly — so the capability existed; `||` was just the broken sugar.

## The fix (DG: option 1, `||`; no new operator; no re-validate)

`transform/statements.rs::lower_list_stmt`: a bless left operand of `&&`/`||` is
rewritten to the if-bless form and lowered via the working `lower_if_bless`:
- `bless V [[t]] || X` → `if bless V [[t]] { } else { X }` (X on fail)
- `bless V [[t]] && X` → `if bless V [[t]] { X }` (X on pass)

Now `bless v [[t]] || v=dflt` gives `v` when valid (clean, blessed) and the
default otherwise (clean literal) — usable either way. Bare `bless` still aborts;
`if bless` unchanged. The fallback is **trusted, not re-validated** (DG: caveat
programmer — a default that wouldn't pass the gate is the author's problem; that
guarantee would have needed the dedicated operator we chose not to add).

## Scope note

Extended to `&&` for consistency (`bless && X` = `if bless { X }`, catches — no
abort on fail). No spec used `bless [[ ]] &&/||` as a list (the `&&` in existing
bless specs is *inside* the `[[ ]]` test), so this breaks nothing; easily
reverted if `&&` should instead abort-on-fail.

## Verified

specs `taint.sh` bfb.1–6 (native,c): valid→value, invalid/empty→default, v
clean+reusable, `&&` runs-on-pass / skips-and-continues-on-fail. 3-mode sweep vs
HEAD `4ede782`: **0 regressions, +12 passing**. DD#24 amended; the
`colon-tilde-elide-vs-bless-fallback` task is closed (`:~` stays DD#84 elide;
no `:!`).
