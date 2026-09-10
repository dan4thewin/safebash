## 212 — redirect fd-dup target validated (SB4048), reject-only

**On `hand-parser`.** DG asked "were there more bugs before resuming
non-executed function checks?" — so I probed the sibling operator-dispatch
parsers for the same silent-degrade class the param-operator work (209-211) had
just closed. Findings:

- **arith** (`$(( x @ 2 ))`) → SB4035/SB4020, **`[[ ]]`** (`[[ 5 <> 3 ]]`) →
  SB4014, **subscript** (`${a[1 2]}`) → SB4041 — all **clean** (reject).
- **redirects** → **BUG**, same class.

### The bug

`>&`/`<&` fd-dup targets were parsed as a general word and never validated.
`emit_dup_output` (codegen/redirects.rs) then did `s.parse().unwrap_or(2)` /
`_ => 2`, so every malformed / non-numeric target **silently dup'd to fd 2**:
`>&foo`, `>&999x`, `2>&1x` (trailing garbage dropped), and `>&-` (bash: *close*
fd 1) all sent stdout to stderr. The compiler was doing something for input it
didn't understand — the exact ambiguity DG has been driving out.

### Fix (DG: reject-only)

`reject_nonfd_dup_target` in `parse_redirect` rejects any `>&`/`<&` target that
isn't a plain non-negative fd number → **SB4048**. `>&N` / `N>&M` / `<&N` still
work; `>&foo` / `>&999x` / `2>&1x` / `>&-` / `>&$n` reject. The `unwrap_or(2)`
fallback is now unreachable (parser guarantees a numeric literal). `RedirectOp`
gained `Copy` (needed to both validate and move it into the `Redirect`).

Deliberately left `>&-` (close) and `>&file` **rejected** for now — DG wants fd
manipulation reconsidered as **builtins** (`dup`/`close`/…), filed
`fd-manipulation-builtins` (ties into the exec/trap-purge "manipulate file
handles of the running program" use case).

### Verification

Specs coverage.sh cov.86-88 (all modes — parse reject). Full 3-mode sweep vs
HEAD `d46d051`: **0 regressions**, +3 each (cov.86-88). fmt + clippy clean.
No redirect DD existed (checked — not deliberate). Bug task marked done.

### Backlog note

The native gate is otherwise clean. The only other latent *native* bug is
`arith-args-subscript-codegen` (a codegen ICE masked by SB5031, unreachable in
practice); everything else open is c/janet baselines. So this was the one native
bug to clear before resuming the never-run grind (`grind-continue`).
