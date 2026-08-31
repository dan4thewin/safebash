title: DD#83 — sash has no here-strings (`<<<`)
status: accepted

Settled with DG (2026-08-30). The here-string operator `<<<` is **removed** from
the language. `producer | consumer` is the one spelling for feeding data to a
command's stdin. `<<<` on any operand is rejected at parse — **SB4050**.

## The thread that produced this

Fixing the redirect-target silent-empty bug (DD-less, journal 215: `> ${x:-f}`
/ `> $a[i]` redirected to an empty filename) raised the same question for the
here-string *data* path (`<<< ${x:-f}` / `<<< $a[i]` / `<<< $(cmd)` all fed
empty stdin). Rather than patch the here-string lowering, DG asked the prior
question: **is `<<< X` ever distinct from `producer | consumer`?**

Working it through (probed against the compiler, not just bash lore):

- **Subshell escape — the reason `<<<` exists in bash — is absent in sash.**
  bash's `read v <<< $x` keeps the reader in the current shell, whereas
  `echo $x | read v` runs the loop in a *subshell* and loses `v`. **sash has no
  subshell**, so `echo $x | read v` already persists `v` (verified). The one job
  `<<<` does in bash is a non-problem here.
- **Clean-vs-tainted stdin is provenance, not channel.** `read v <<< $x` yields a
  clean `v` for clean `x` — but so does `echo $x | read v` (inline `echo` =
  clean); `/bin/echo $x | read v` and `<<< $(extcmd)` are both tainted. The
  cleanliness never came from the here-string; it came from the *source* being
  inline/literal. Pipes and here-strings taint identically.
- **No word-splitting in sash**, so there is not even a quoted/unquoted subtlety:
  `<<< $x` == `echo $x |` == `echo "$x" |`, byte for byte (modulo a trailing
  newline the here-string was *buggily* dropping).
- What remained — buffering-vs-streaming and failure-timing — are either
  performance or *arguments against* `<<<` (a command buried inside a
  here-string-word expansion has murkier DD#55 status than an explicit pipe
  stage).

Conclusion: `consumer <<< X ≡ echo-of-X | consumer` for **every** operand, with
no semantics a pipe lacks. `<<<` is pure sugar, and sash keeps one spelling.
Dropping it also retires two here-string bugs (silent-empty data, missing
trailing newline) instead of fixing dead-on-arrival machinery.

## Decision

1. **`<<<` is rejected at parse — SB4050** — with a message that names the
   rewrite (`cat <<< $x` → `echo $x | cat`; `read v <<< $x` → `echo $x | read
   v`). The `<<<` token is still lexed so the diagnostic is precise rather than a
   generic syntax error.
2. **`RedirectOp::HereString` is removed** from the IR, and with it all the
   downstream machinery: native codegen `emit_herestring_redirect` +
   `sash_herestring_redirect` extern/runtime symbol; the `emit_c` emitter +
   extern; the whole `emit_janet` here-string family; the validate taint
   exception (`read/mapfile <<< literal` → clean) and the `ssa_taint`
   clean-literal source. `read` is clean now only via a clean pipe.
3. **The `SB5002` heredoc message** no longer points at here-strings; it points
   at pipes.

## Consequences

- The bash idiom `while read …; done <<< "$(cmd)"` becomes `cmd | while read …`
  (which works in sash — no subshell). A single capture is `v=$(cmd)`.
- **Converter**: bash `<<<` must be rewritten to a pipe/capture on conversion —
  filed as `converter-herestring-to-pipe`.
- Supersedes the just-filed `bug-herestring-nonscalar-data-empty` (the construct
  is gone, so the bug is moot).

## Related

Journal 216. Follows the same "reject complex / one clear spelling / no silent
behavior" line as SB4049 (redirect targets must be scalars, journal 215),
DD#65 (var-modifier prune), DD#70 (`!` must-consume).
