title: A discarded status has no consumer
status: complete

DG chose **reject** on 2026-11-08 for testing a result after `-` discards
it. Copilot recorded the rule in DD55/DD70/DD74/DD88 and GUIDE, closed
Q3's policy question in `q-suppression-and-stage-residuals`, and added the
P3 implementation bug `reject-discarded-status-consumers`. The other
questions in that record remain open.

The discussion used two spellings:

```bash
if - /bin/false { echo yes }
- /bin/false || echo recovered
```

Both require refusal. The preceding adjacency ruling identifies the
command whose result is discarded; this ruling says a consumer cannot
then use that result. Neither an always-successful replacement nor the
historical single-command/pipeline split was selected.

The rule applies to `if`, `elif`, `while`, `until`, logical-list
decisions and capture conditions. DD74 already identifies the recursive
contexts: a middle list clause supplies the next connector, and a final
clause supplies an enclosing condition. Thus `a || - b` remains legal
as a statement but not as the condition of an `if`.

An all-suppressed pipeline cannot supply a consumed status. A partially
suppressed one still supplies the aggregate over its unsuppressed stages;
this is not a ban on per-stage discard in conditions. The all-suppressed
statement form remains legal.

DD74 also explicitly makes `return <producer>` a status consumer, so
the rule applies there too, including a discarded list tail or an
all-suppressed pipeline. That consequence was reported while recording.
A function may still discard a command and then return a fresh authored
status. `- cmd; return 0` is not forwarding the discarded answer, and
the accepted constant-result rule remains intact.

The consumer must be identified semantically, not by finding any minus
inside a condition. A legal `$(- cmd)` text operand supplies stdout;
the enclosing test consumes its own boolean, not that command's status.
Taint and type rules still apply. Nor may a caller's predicate context
leak into a called or captured body.

Ordinary condition and return handling disables implicit aborts because
it consumes status. It is not source discard, even where lowering shares
unchecked/suppressed flags. DD88's implementation explanation now makes
that distinction explicit rather than presenting a capture condition as
source-equivalent to `x=$(- cmd)`.

Process exit bookkeeping and saved child outcomes are not reset by this
rule. In particular, `- wait` discards the returned pid, while DD126's
`waitstatus` is a separate output. Existing inversion, result-kind,
status-availability, no-reach and failure-class rules remain.

The two suppression implementation tasks now coordinate list binding and
consumer refusal, including review of B84's 55.145-55.147 at fresh ids
where contracts change. The historical non-status-condition task no longer
advertises a directly suppressed command as a valid condition, or its
pre-DD88 blanket rejection of capture conditions as current guidance.

The old probe table was not rerun or revalidated against later language
changes. No compiler, runtime, spec or build code changed; no new
behavior probe, full sweep or coverage run was made. Journal 577 remains
the accepted implementation baseline. Enforcement and its focused
before/reversion controls remain queued.
