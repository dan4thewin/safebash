## 216 — drop here-strings (`<<<`) entirely (DD#83)

**On `hand-parser`.** DG ruled `<<<` out of the language. Follows directly from
the redirect-target work (journal 215): the here-string *data* path had the same
silent-empty class, and probing it raised the deeper question — **is `<<< X`
ever distinct from `producer | consumer`?**

### The finding (probed, not assumed)

Worked through with DG against the actual compiler:

- **The subshell escape that justifies `<<<` in bash is absent in sash.** bash's
  `read v <<< $x` keeps the reader in the current shell; `echo $x | read v` runs
  it in a subshell and loses `v`. **sash has no subshell**, so `echo $x | read v`
  already persists `v` (verified). `<<<`'s reason for existing is a non-problem
  here.
- **Clean-vs-tainted stdin is provenance, not channel.** `read v <<< $x` is clean
  for clean `x` — but so is `echo $x | read v` (inline echo); `/bin/echo $x |`
  and `<<< $(extcmd)` are both tainted. The cleanliness came from the source, not
  the here-string. My earlier "here-strings uniquely deliver clean stdin" claim
  was wrong (I'd compared against an *external* `printf`).
- **sash doesn't word-split**, so there's not even a quoted/unquoted subtlety:
  `<<< $x` == `echo $x |` == `echo "$x" |` byte-for-byte (modulo a trailing
  newline the here-string was buggily dropping).

Conclusion: `consumer <<< X ≡ echo-of-X | consumer` for every operand, no
semantics a pipe lacks. Pure sugar → dropped. (Also retires the silent-empty
data bug and the missing-newline bug instead of fixing dead-on-arrival code.)

### Change

**Parse:** `<<<` → **SB4050** with a rewrite-naming message (`cat <<< $x` →
`echo $x | cat`; `read v <<< $x` → `echo $x | read v`). The `<<<` token is still
lexed so the error is precise.

**Removed `RedirectOp::HereString`** and all downstream (the compiler's
exhaustiveness check drove the sweep — 9 sites):
- native codegen: `emit_herestring_redirect`, the dispatch/flags arms, the
  `sash_herestring_redirect` extern (setup.rs, types.rs) **and the runtime
  symbol** (sash-rt/src/lib.rs);
- `emit_c`: emitter + extern + arms;
- `emit_janet`: the whole here-string family (has_/collect_/emit_redirected_
  herestring[_cat|_read|_generic], is_cat_no_args, extract_command_name_and_args)
  — `emit_redirected` collapses to "emit body + comment"; kept the shared
  `escape_for_janet_string`;
- `validate`: the `read/mapfile <<< literal` taint exception (taint_redirected
  simplified) + the SB5002 heredoc message now points at pipes;
- `ssa_taint`: `read_line_taint` drops the `from_literal` source (a `read` is
  clean only via a clean pipe now);
- `transform`: the spill no longer special-cases here-strings (all redirects are
  file redirects).

### Specs

- `commands.sh` 14.25 (plan 39→40): `<<<` → SB4050, all modes.
- `taint.sh` 24.6 rewritten from `read x <<< "hello"` to `echo hello | read x`
  (still "clean source → clean read", now via the pipe idiom).

### Verify

Full `SPEC_MODES="native c janet"` sweep, latest-run-per-test diff vs `b8d2a13`:
**0 regressions all three modes, +1 (14.25) each**; 24.6 stayed passing. fmt +
clippy clean (fixed a clippy `match`→`if` on the now-single-arm redirect
dispatch). Probed: `<<< X` rejects in native/c/janet; `echo $x | read v` clean +
persists.

### Bookkeeping

DD#83 written. `bug-herestring-nonscalar-data-empty` → superseded.
`converter-herestring-to-pipe` filed (P3) — bash `<<<` must be rewritten to a
pipe on conversion.
