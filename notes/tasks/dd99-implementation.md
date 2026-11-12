title: "DD#99 implementation — read/readline, NUL-only, no flags"
status: done
priority: P1
kind: task
depends: []
gates:
  - "`readline v` reads one line into a scalar; `read a` reads NUL-separated fields into an array; `read $s a` splits a scalar"
  - "`map m = @{a:DISP}` builds a map pairwise, disposition mandatory; `array a = @m` is rejected"
  - "`write` is echo-without-newline; `echo -n` is rejected"
  - "read -a / -f / -d / -r, mapfile, readarray and multi-target `read a b c` are all rejected"
  - "an argv entry containing an interior NUL aborts at runtime rather than truncating silently"
  - "3-mode sweep with 0 regressions at every stage"

# Staged plan

DG (2026-09-09): *"maybe call it `newread` instead of `read` — finish
implementing `readline` and `newread` and then replace."*

The array read ships under a **temporary name**, so at no point does an
existing spelling change meaning while code still depends on it. The old
`read` keeps working untouched until everything has moved off it.

| # | stage | migration load |
|---|---|---|
| 1 | add `readline v` (one line → scalar) ✅ | — additive |
| 2 | add `newread a` / `newread $s a` (NUL fields → array) | — additive |
| 3 | add `map m = @{a:DISP}` (pairwise, disposition mandatory) | — additive |
| 4 | add `write` (echo without the newline) | — additive |
| 5 | migrate spec/ + contrib/ onto the new spellings | ~250 sites |
| 6 | retire `read -a/-f/-d/-r`, `mapfile`, `readarray`, multi-target read; **rename `newread` → `read`** (`echo -n` already done in stage 4) | — |
| 7 | argv interior-NUL runtime guard | — |
| 8 | `array a = @m` → compile error | — |

Scale: ~250 sites across 25 of 38 spec files.

Stages 1–4 are purely additive and independently verifiable. Stage 6 is the
only breaking one, and by then nothing depends on the old spellings.

## Known blockers, all filed and independent

These are broken today and the design leans on them:

- `bug-while-read-a-zero-iterations` — `while read -a` runs zero iterations
- `bug-sed-op-on-array-empties-it` — `a =~ s///` silently empties an array;
  `gsub @a` silently joins the splat
- `bug-match-outarray-no-autodeclare` — `match S P ARR` needs a prior decl

Stage 3 subsumes the first (the `-a` shape disappears); the others are
independent.

## Progress

- **Stage 1 DONE** (2026-09-09): `readline v` added — one line into one scalar,
  no flags, exactly one bare target (SB5057). It shares the scalar lowering
  with today's single-target `read`, so stage 3 can give `read` the array-only
  form without the two meanings colliding. Specs `wordsplit.sh` rl.1–rl.10.
  Sweep 4376 passing / 39 failing / 405 skipped, 0 regressions.
  - rl.7 (`while readline`) is gated native-only: the identical `while read v`
    shape also fails in c/janet, so that is a pre-existing backend gap.
- **Stage 2a DONE** (2026-09-09): `newread ARRAY` — reads stdin, splits on
  NUL, fills an auto-declared array. No flags, exactly one bare target
  (SB5057, shared with `readline`). Lowers to the existing `ReadArray` with
  `delim = Some(0)`. Specs `wordsplit.sh` nr.1–nr.8, including nr.5 — the full
  DD#99 round trip with no external `tr`:

  ```sh
  sep=$'\0'; s=a:b:c; s =~ s/:/$sep/g
  echo $s | newread f          # → 3 elements
  ```

  Sweep 4395 passing / 39 failing / 410 skipped, 0 regressions.
- **Stage 2b DONE** (2026-09-09): `newread $SRC ARRAY` — splits a scalar,
  DG's "`split` is just a two-argument `read`". New `SsaInst::SplitValue`
  (native + c; janet inherits via the shared front end), reusing
  `sash_split_into_array` with the bytes from `lower_value` instead of a fd.
  The **sigil rule is enforced**: the source must be a value (`$s`) and the
  target a bare name, so `newread f g` is SB5057. Specs nr.9–nr.14.
  Sweep 4409 passing / 39 failing / 414 skipped, 0 regressions.
- **Stage 3 DONE** (2026-09-09): `map m = @{a:DISP}` — pairwise array→map.
  New runtime `sash_map_from_array`, `SsaInst::MapFromArray`, native + c
  emitters, and **SB5058** for a bare `map m = @a` (which used to compile and
  silently yield an EMPTY map). `:-KEY` puts the trailing element at `${m[KEY]}`
  — DG's landmark; `:?MSG` aborts at runtime; `:~`/`:!` discard; an empty array
  is a valid empty map. Specs `var_modifiers.sh` mp.1–mp.9, including mp.8 —
  the whole map pipeline in-process, `k:v` text → NUL → array → map.
  Sweep 4429 passing / 39 failing / 421 skipped, 0 regressions.
  - janet has no emitter for the fill; filed `janet-map-from-array`, specs
    gated native,c. The SB5058 reject and the empty case work in all 3.
- **Stage 4 DONE** (2026-09-09): `write` — echo without the trailing newline
  (DG: *"I'd spell echo -n as write"*). Desugars to `echo -n` in the transform,
  so native and c get it from the shared SSA lowering; registering it under
  `BuiltinCmd::Echo` is what stops resolution shelling out to `/usr/bin/write`.
  Specs `commands.sh` wr.1–wr.7. Sweep 4446 passing / 39 failing / 425 skipped,
  0 regressions.
  - **`echo -n` is retired now, not in stage 7** — only one live site existed
    (`wordsplit.sh` 22.31), so leaving two spellings around was not worth it.
    Source-level `echo -n` is **SB5059**; `-n` survives only as the internal
    SSA spelling, which is why the check runs with the *structural* pass,
    before the rewrite that synthesises it.
  - janet renders from the AST, so it never sees the SSA rewrite; filed
    `janet-write-builtin`, specs gated native,c.
- Stage 5: next — migrate spec/ + contrib/ onto the new spellings (~250 sites)

## Stage 5 progress (2026-09-09) — partial, blocked

Real inventory is **88 sites**, not the ~250 first estimated (that count was
inflated by prose in comments).

| group | n | status |
|---|---|---|
| single-target `read v` → `readline v` | 13 | **done** |
| `read -a X -d ""` / `read -a X` used as a VEHICLE → `newread X` | 12 | **done** |
| `read -a`/`-d`/`-r`/`-z`/`-f` where the FLAG is the test's SUBJECT | 14 | deferred to stage 6 (they must become reject-tests, and the rejection does not exist yet) |
| `mapfile -t` (lines → array) | ~19 | **blocked** |
| `mapfile -m` (`k=v` lines → map) | ~20 | **blocked** |
| multi-target `read a b c` | 9 | deferred to stage 6 |

The classification rule used: if the removed spelling is the **subject** of the
spec it becomes a reject-test in stage 6; if it is merely the **vehicle** for
testing something else, it migrates now so the original subject stays covered.

### Found while migrating

`newread`/`readline` were not wired into the `args` write-protection —
`newread args` was accepted where `read -a args` is SB5052. Fixed (spec 84.26
caught it, because migrating the vehicle turned a passing test red).

### Blocker

`bug-regex-blind-past-interior-nul` (P1, filed). Every regex-backed string op
reads its subject as a C string and silently ignores the bytes past an interior
NUL. The `mapfile` replacement is "substitute the separator to NUL, then
split", so any replacement needing **two** edits — which `mapfile -m` does,
one for the line separator and one for the first `=` of each record — is
silently wrong today.

`mapfile -t` is also blocked in practice: the in-process spelling has the same
shape, and DG's external one-liner `tr '\n' '\0' < f | read arr` only covers
the cases that read from a file.

## Stage 5 progress (2026-09-09, part 2)

`matchall` (DD#63) unblocked the mapfile work, and the pipeline-stage redirect
fix unblocked DG's one-liner. Two distinct replacements, and **which one you
want depends on the taint story**:

| shape | replacement | resulting taint |
|---|---|---|
| `mapfile -t a < f` | `/usr/bin/tr "\n" "\0" < f \| newread a` | `a` **tainted** |
| (in-process) | `s=$(<f); attest s […]` then `- matchall $s "[^\x0a]+" a` | `a` **clean** |

Both are correct; they differ in where the vouching happens. The pipeline route
fills the array directly from external input, so the array carries the taint
and must be attested before use. The `$(<f)` route reads the file into a scalar
first — an ordinary read, so it must be vouched for there, and everything
derived is then clean.

**24 `mapfile -t` sites migrated** to the pipeline route, which is the one that
preserves what the taint specs actually test. taint.sh 178/178.

## Blocked on a decision: is a tainted MAP still constructible?

`mapfile -m` was the only primitive that produced one. Its DD#99 replacement is
array→map, and `map m = @{a:DISP}` is today an ordinary **read** of `a` —
measurably identical to `join @a` and `for x in @a`, all three SB5020 on a
tainted array. So the array must be attested first, which cleans it, and **the
resulting map is always clean**.

That makes the tainted-map scenario unreachable, and 8 specs test it (60.28,
60.29, 60.30, 60.31, 60.34, 60.35, 60.36, 60.48).

The fork is whether `map m = @{a:DISP}` is a **read** of the array (today —
DD#24 no-propagation applies, map is clean) or a **bulk transfer** like
`newread` (taint flows, map stays tainted, attest-all on the map survives).
DD#24 settles the behaviour of a read; it does not settle which of the two this
operation *is*. Awaiting DG.

## Stage 5 COMPLETE (2026-09-10)

DG settled the fork: *"the read of the array must have a clean array, so no
tainted maps."* `map m = @{a:DISP}` is an ordinary read, DD#24's
no-propagation applies, and the tainted-map scenario is gone from the
language. The taint gate does not disappear — it moves one step earlier, onto
the array, and it is the *same* gate.

All 39 `mapfile` sites migrated; **no live `mapfile` use remains in spec/**.

| shape | replacement |
|---|---|
| `mapfile -t a < f` (taint is the subject) | `/usr/bin/tr "\n" "\0" < f \| newread a` |
| `mapfile -t a` (splitting is the subject) | `- matchall $s "[^\x0a]+" a` |
| `mapfile -m m` | `- matchall $s "([^=\x0a]*)=([^\x0a]*)" p` + `map m = @{p:~}` |

60.34/60.35/60.49 and 22.11 were rewritten rather than deleted: they now pin
the relocated gate (a tainted array cannot be paired into a map) and its
positive counterpart (once vouched for, the map is clean).

### Two real defects found by migrating

- **`matchall` was missing from the `args` write-protection.** `matchall … args`
  was accepted where `newread args` is SB5052. It was masked by SB5050 ("dead
  variable") firing first, which is also the wrong reason — a write to `args`
  is illegal whether or not it is read — so `args` is now exempt from SB5050
  and the accurate rule reports. Specs `optional_args.sh` 84.28, 84.31, 84.32.
- Two of my own rewrites (22.11, 22.13) replaced a tainted producer with a
  literal scalar, which would have **laundered the taint tests into
  tautologies**. Caught by the specs going red; restored to external producers.

### Cost

Five janet specs went passing→skipped (`wordsplit.sh` 22.6/22.8/22.9/22.10,
`misc.sh` 54.1): janet does not emit `map m = @{a:DISP}`, so it cannot build a
map from external data at all. Task `janet-map-from-array` raised to P2.

**Next: stage 6** — retire `read -a/-f/-d/-r`, `mapfile`, `readarray` and
multi-target read (the 14 flag-subject + 9 multi-target specs become
reject-tests), rename `newread` → `read`, and update **sash-convert**, which
still emits `read -r line`.

## CLOSED 2026-09-10 — every gate verified at HEAD

| gate | result |
|---|---|
| `readline v` / `read a` / `read $s a` | `hi` / 2 fields / 1 field |
| `map m = @{a:DISP}`; `array a = @m` rejected | `v` / **SB5061** |
| `write`; `echo -n` rejected | `abc` / **SB5059** |
| `read -a`/`-f`/`-r`, multi-target, `mapfile`, `readarray` rejected | **SB5057** ×4, **SB5060** ×2 |
| argv interior NUL aborts | aborts with a message, rc 1 |
| 3-mode sweep, 0 regressions at every stage | held throughout (4120 → 4599 passing) |

Stages 1-8 complete. Two follow-ons are tracked separately and are **not**
part of this task: `sash-convert-read-dd99` (P1 — the converter still emits
`read -r`, so its output no longer compiles) and `docs/sash.1`, which still
documents the retired flag surface.
