# 167 — janet structural parse-map, stage 1

DG re-aimed janet (DD#15 amendment): a 1-to-1 map of how the compiler *parsed*
the sash, readable side-by-side — "as though I'd written sash in sexpr." Not
the current idiomatic-janet lowering (which dissolves `$()` into `os/spawn`,
elides `bless` to a comment, and inlines status threading).

DG's directives: build it running (skip paper), in stages, for an early read;
`bless` and similar present + *functional* (gate → abort/else) but no
clean/tainted flag on vars (taint is compile-time).

## Shape

`sash/*` vocabulary over a `sash-rt.janet` primitive library; semantics in the
library, the emitted map stays structural. Two janet facts unlocked a clean
design: janet allows `(defmacro sash/if …)` top-level slash names, and `$` is a
legal symbol — so `(sash/echo ($ x))` works in a self-contained preamble (no
module/path setup; `janet foo.janet` just runs).

**Position-based interpretation is made visible** — a bare checked command is
`(sash/run …)` (aborts on nonzero, DD#55), the same command in an if/while
condition is `(sash/ok? …)` (caught), and `- cmd` is `(sash/dash …)`. That is
exactly the "how the compiler interprets" DG wanted to see.

Control flow uses thin macros (`sash/if` = 1-line alias to janet `if`): emit
still supplies the branch structure from the IR (IR-stage oracle independence
kept), the macro only gives it a readable name.

## Stage 1 (behind `--janet-map`, additive — old `--janet` untouched)

`x=5; echo $x; if [[ $x = 5 ]]; then echo yes; else echo no; fi;
y=$(/bin/echo hi); bless y [[ -n $y ]]; echo $y; /bin/true; echo done`
renders to:

```janet
(set x "5")
(sash/echo ($ x))
(sash/if (sash/= ($ x) "5") (do (sash/echo "yes")) (do (sash/echo "no")))
(set y (sash/capture "/bin/echo" "hi"))
(sash/bless-checked (sash/-n ($ y)))
(sash/echo ($ y))
(sash/run "/bin/true")
(sash/echo "done")
```

…and runs (5/yes/hi/done). Covers assign, echo, `:`, external run/ok?, if/elif/
else/while/until, `[[ ]]` string+numeric tests, `$(single-external)` capture,
bless, `- cmd`, `$?`. Unhandled → visible `(sash/TODO …)`. Validates through
the real front-end (SB5031 rejects unset-var reads before a map is emitted).

Nil-coalesce on reads (`($ x)` → `(or x "")`) so a var read before its
assignment executes reads empty.

## Files

- `sash/src/emit_janet_map.rs` — the structural emitter.
- `sash/janet/sash-rt.janet` — stage-1 primitive library (prepended preamble).
- `sash/src/lib.rs` — `--janet-map` flag + module.
- DD#15 amendment; `notes/tasks/janet-structural-map.md` (staging).

## Next

Await DG's read/experimentation, then S2 (functions/positionals) etc. Open
aesthetic refinements (preamble vs import, `($ x)` vs bare `x`) deliberately
left for DG to react to by editing + running.
