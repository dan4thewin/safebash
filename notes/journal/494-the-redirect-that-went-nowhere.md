# 494 — The redirect that went nowhere

The coverage queue asked about `argv_splat_start`'s three cold lines. The
answer was a bug, then a wider bug, then a compiler crash, and by the end a
rule that had been missing since redirects were implemented.

## What the cold line was hiding

`argv_splat_start` looked *through* `StmtKind::Redirected`. So
`for x in $(argv >f)` compiled, iterated the arguments, and never created the
file. The filename did not reach the binary at all. Both backends agreed, so
it was one fault in the shared front end rather than a divergence.

This was the surviving half of a bug fixed once already. `$(argv; touch f)`
had silently discarded the `touch`; the repair narrowed the transform to a
single statement, which closed the shape anyone had thought to test and left
the redirect shape three lines below. `av.c3` *looks* like it covers the
redirect case and does not — its body is `argv 2>/dev/null; /bin/echo extra`,
so it was rejected for being two statements. The redirect never had a spec of
its own, which is exactly why it survived the fix that closed its twin.

DG ruled: reject it, same rule, same code as the multi-statement body.

## Then a spec I wrote failed

`av.c13` asserted the other side of that rule — DD#60's out-param fill *is* a
statement, so I claimed a redirect on it was meaningful and stayed legal.

It failed. `argv 1 a >f` does not honour its redirect; it drops it.

I had written the spec from an unverified assumption. I had checked the fill's
*output* and never checked whether the file appeared. The spec failing is what
turned a one-line fix into the rest of this entry — **a spec that fails is a
finding**, and this one was reporting that my model of the code was wrong.

## The fault was systemic

Measured across seventeen builtins, in positions where each is otherwise
legal:

| statement | sash | bash |
|---|---|---|
| `echo`/`write`/external/user fn/pipeline | honoured | honoured |
| `: >f` | **dropped** | creates |
| `cd /tmp >f` | **dropped** | creates |
| `exit 0 >f` | **dropped** | creates |
| `close 9 >f` | **dropped** | n/a |
| `argv 1 a >f` | **dropped** | n/a |
| `unset m[k] >f` | **dropped** | n/a |
| `return 0 >f` | **dropped** | creates |
| `if match … >f` | **dropped** | n/a |
| `exec /bin/echo hi >f` | **dropped, printed to the terminal** | writes to the file |

Two independent causes, which is why no single patch would have done it:

1. `attach_redirects_to_last_inst` ended in `_ => {}`. Anything that was not
   `Command` / `ReadLine` / `ReadArray` / `Pipeline` had its redirects
   dropped on the floor. **Fourth instance in four sessions of a wildcard
   over an IR enum being the bug.**
2. Several `emit_command_kind` arms — `Noop`, `Cd`, `Exit`, `Close`, `Wait`,
   `Match` — take no `redirects` parameter *at all*. The SSA carries them
   faithfully; codegen never looks.

`exec` was the worst of them, because it is the only one whose failure is
**wrong output** rather than a missing side effect. `emit_exec` passes
`redirects` to the no-args branch (`exec >f`, the fd-manipulation form, which
works) and not to the argv branch, so `exec /bin/echo hi >f` printed `hi` to
the terminal while the file was never made.

## The c backend had been right the whole time

`emit_c`'s `emit_command` wraps *every* command body in its redirect guard, so
c-mode honoured `: >f` and `exec CMD >f` correctly. Native did not.

The two backends had been disagreeing for as long as the feature has existed,
and the differential oracle never said so, because **the oracle can only
report a divergence on a program some spec actually runs.** A gap in the suite
is a gap in the oracle. That is worth remembering every time the dual-mode
sweep comes back green: it is evidence about the programs in `spec/`, not
about the language.

## The ICE

While enumerating, `attest s [[ -n $s ]] >f` **panicked the compiler**. All
three redirect operators.

`parse_attest` builds its `StmtKind::Command` by hand and never routes through
`parse_simple_command`, so it is the one command that reaches
`wrap_pipeline_part` still holding its trailing redirect.
`noncommand_redirect_target` has no arm for it, returns `None`, and
`redirect_reject_noun` panics.

That ICE's doc said:

> Panicking instead turns that regression into a caught ICE. Exercised by
> `redirect_reject_noun_is_loud_ice`, since no source text can reach it.

The first sentence was right and the second was false. It was placed to catch
precisely "some statement kind later gains the ability to carry a redirect
past `parse_simple_command`" — and that is exactly what `parse_attest` is.
The ICE did its job; nobody had run the program that fires it.

The unit test could not have found this. It constructs the one shape it
already knows about, so it re-asks the question the arm was written to answer
rather than asking whether any *source* reaches it. This is the shape of
claim-in-a-comment that AGENTS.md warns about: written down, true-sounding,
and never checked again. The doc now records the falsification.

Structurally bounded, which is what made it safe to stop looking: there are
exactly three `StmtKind::Command(Command {` constructors in the parser.
`parse_test_command` is covered by `is_bracket_test`, `parse_command` absorbs
its own redirects, and `parse_attest` was the gap.

## The rule

DG: **reject all except `:` — make `: >f` work.** Then, on `return`:

> the redirect there is only in conjunction with a command/function/pipeline
> (like a normal statement). return N specifies an explicit status value while
> the other forms derive the value. return N >f is nonsense

The gate is a closed allow-list, `BuiltinCmd::may_take_redirect`, total over
the enum and marker-enforced, applied in `parse_command`. A redirect survives
parsing only on `echo`, `write`, `exec`, `:`, `read`/`readline`, an external
command or a user function — which is exactly the set that lowers to the four
arms that can carry one — or on a pipeline.

Putting it upstream of *both* failure sites is the point. Patching the
transform would have left the codegen arms; patching codegen would have left
the transform. With the gate at parse, neither site can be reached holding a
redirect it would discard, and `attach_redirects_to_last_inst` is now total,
its ICE arms naming the rule that makes them unreachable.

`: >f` honours its redirect through the same guarded-body wrapper `echo` uses,
which gives it DD#55 open-failure behaviour for free. `exec CMD >f` installs
the redirect unguarded and then replaces the process, so the exec'd program
inherits it; it deliberately skips the DD#120 `scope_bind` rebind that the
no-argument form does, because a per-command redirect is a channel for the
command's output, not for the program's diagnostics about it.

## DD#74, read precisely

Two existing specs, `74.63` and `74.64`, asserted `return 0 2>/dev/null`
compiles. `74.64`'s own comment explained that the redirect "has no inst of
its own to attach to … so the attach step has to leave it alone" — the drop,
written down and accepted rather than reported. A fault that is written down
is still a live fault.

DD#74 says `return` absorbs "a pipeline, an `&&`/`||` list, or a trailing
redirect — exactly as a normal statement would", which reads like a ruling in
favour of the old behaviour. It is not, and the distinction is DG's:
`return N` *states* the status, every other form *derives* it from a producer
that does produce output. DD#74 itself carves this out — "`return N` stays a
`Command`" — and a `Command` on a no-output builtin is what the new rule
rejects. Verified both ways: `return /bin/true 2>/dev/null` still compiles and
honours its redirect; `return 0 2>/dev/null` is SB4046.

I did not decide this one. Overturning a spec that encodes a deliberate
choice is the case AGENTS.md says to take to DG, and I took it.

`74.63` moved to the producer form it was really testing (the predicate walk
seeing through a `Redirected`), `74.64` inverted, `74.64b` added so the value
still reaching `$?` stays pinned.

## Also

`keys_splat_map_name` was a private fork of the shared `stmt_command` walker,
still carrying `stmts.first()` — the spelling `sole_cmdsub_command` was
*fixed away from* when `$(argv; touch f)` turned out to discard the `touch`.
The caller's guard already required a sole statement, so the copy was wrong
and harmless simultaneously, which is how a fork survives review. Third
instance in three sessions. `keys_splat_command` is now the single authority
and `is_keys_splat_expr` is defined in terms of it.

The SB5025 arm I added last session for `$(argv >f)` is **removed**, not left
behind: SB4046 now rejects that program one pass earlier, so the arm could
never run again. Leaving it would have been an unreachable claim pretending to
be a rule.

## Gates

Sweep **8227**, predicted exactly (native 4216, c 3998, 13 skipped). Zero
regressions — every flip failing→passing. Exactly one new id per mode
(`74.64b`). Liveness 258, so the comparison was capable of failing. 150 unit
tests, fmt / clippy / totality / complexity green.

Commits `7c08b55` (the keys fork) and `c44b2cf` (the redirect rule).

## What to carry forward

- **A spec that fails is a finding.** `av.c13` was written from an assumption
  I had not checked, and its failure is the only reason any of this was found.
- **A green dual-mode sweep is evidence about `spec/`, not about the
  language.** The backends had disagreed for the life of the feature.
- **An ICE's "unreachable" claim decays like any other comment.** A unit test
  that constructs the known shape cannot renew it; only enumerating the
  producers can.
- **Fix upstream of every drop site, not at the drop sites.** Two independent
  causes meant either patch alone would have looked like a fix and left half
  the bug.
