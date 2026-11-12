title: "Reject process-replacing exec inside stdout captures"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "Direct and transitive exec PROGRAM in stdout-capture bodies receive a located compile-time refusal in native and C, independently of taint switches"
  - "Capture placement, conditional paths, wrappers, nested captures and forked child bodies cannot bypass the prohibition"
  - "Ordinary external captures, fd-only exec, exec outside captures and DD58's direct integer-value-call contracts are preserved"
  - "Changed cap.35/cap.36 contracts use fresh global ids; focused before/reversion evidence and a full gate pass with measured predictions"

## Decision settled; enforcement pending

DG chose **"reject exec within capture"** on 2026-11-08. DD86 records the
accepted rule; DD55, DD58 and GUIDE agree. This is batch-531's B34
(`batch-531-findings.md`, section 12). The existing slug and P3 priority
are retained, but this is now an implementation bug, not an unanswered
policy question. Journal 587 records the choice.

Reject the source command `exec PROGRAM ...` in a stdout-capture body,
including an anonymous body or a captured named function. Follow calls
transitively: a helper, including an integer helper evaluated inside the
body, must not hide the forbidden operation. The restriction applies in
assignment, argument and interpolation positions and through control flow,
redirects, suppression, nested captures and forked child bodies. It is
structural and independent of taint, not conditional on proving that one
particular execution loses output.

This does **not** reject ordinary external commands because the runtime
starts them with `execve` in a child. It rejects the explicit source-level
process-replacement operation. The fd-only form, such as `exec >file`,
remains legal under the existing descriptor rules. Ordinary `exec PROGRAM`
outside stdout captures retains its contract.

Keep DD58's semantic distinction: a direct integer-value call is not a
stdout-capture body just because its spelling uses `$()`. The integer and
arithmetic spellings stay uniform, through redirects too. Conversely, an
integer call evaluated *inside* a stdout-capture body does not escape that
body's restriction. Do not copy the existing redirect-dependent kind bug
(`bug-int-capture-redirect-switches-kind`) into this check.

Do not fork a body to make an otherwise forbidden `exec` work. The
capture's global effects, ordinary command forks and result channels stay
unchanged. This is not a blanket prohibition on non-returning paths;
DD128's `exit` and `fail` remain separate, unchanged operations.

## Historical evidence at filing

The following is the original fixture and its recorded native/C result,
not a new measurement against the current typing/taint rules:

```
echo before
str x=$(exec /bin/echo hi)
echo "after [$x]"
```

The program printed `before`, then exited 0 without printing `after`. `hi`
went into the capture, which never completed, so nothing read it. Calling
a function from the capture gave the same result
(`fn f { exec /bin/echo hi }; str x=$(f)`). In this main-process example,
the process that would finish the capture is the one `exec` replaces.
The recorded Bash comparison forks the capture and prints `after [hi]`;
DG did not choose that model for sash.

cap.35 and cap.36 pin the old acceptance/transport behavior. Enforcement
must retire those contracts in favor of located refusal checks at fresh
ids, not silently rewrite their history.

Until journal 546 the output also depended on which capture path the site
took. The purity walk (`capture_purity.rs`) took the bare builtin `exec` for
an in-process write, so a body with nothing else in it took the userspace
buffer path, and the exec'd command wrote `hi` to the program's real stdout
(`before`, `hi`, measured at ab133dd2). A body made impure by an unrelated
external (`fn f { /bin/true; exec /bin/echo hi }`) already printed `before`
only. Journal 546 marks `exec` impure, so both paths agree. That is a fix to
an optimisation, which must be unobservable; it did not itself reject the
source operation.

At filing, hand-parser accepted the example with taint on, while the
reference branch reported SB5020 under its old B114 output classification.
Neither result was enforcement of an `exec` prohibition. Revalidate
fixture legality when implementing this task; an unrelated taint or
return-kind refusal is not coverage of the new rule.

The fd-only control is not affected. `exec >/dev/null` in a capture only
rebinds this process's descriptors. It parses as a redirected statement,
which the purity walk already treats as impure. The recorded controls
print `[]` then `after` in both backends.

Probes: the session archive `files/port-f5-probes.tar.gz` (`q/b34a.sh`-
`b34c.sh`; `px/p1.sh`-`p4.sh` for the two capture paths, `px/f1.sh`-`f4.sh`
for the fd-only form).

## Implementation and regression obligations

Use the shared body/call inventory to distinguish the two `exec` forms and
carry the process-replacement fact through calls. Do not add a private
walker or assume the taint summaries already encode this fact. A diagnostic
for an indirect case should identify the capture and the offending `exec`,
with the call path needed to understand their connection.

Add focused direct, nested and transitive-call refusal cases, including
an integer helper called within a stdout capture. Cover the otherwise
admitted wrapper/process contexts rather than letting an earlier unrelated
diagnostic satisfy the assertion. Assert the rule's code and distinguishing
location/message, and retain taint-disabled controls.

Preserve ordinary external captures, fd-only redirection, legal global
effects and `exec` outside captures, including an ordinary call to a helper
whose captured use must reject. Keep direct integer-value calls consistent
across spellings and redirects. Reuse existing controls where they already
pin these claims instead of adding rival tests.

Before/reversion evidence must show that the new refusal is doing the
work. Measure all affected specs before predicting the full gate; this
recording makes no sweep-count or zero-impact prediction. No compiler,
runtime or spec implementation has been made by accepting the decision.
