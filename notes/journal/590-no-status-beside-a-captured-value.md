title: No status beside a captured value
status: complete

DG chose **reject** for `-` on a value-intrinsic capture with no separate
status, then requested the next decision. The example was:

```bash
str x=$(- toupper abc)
echo "$x"
```

The capture consumes the string value; the prefix has no status left
to discard. The accepted spelling is `$(toupper abc)`, without `-`.
Keeping a redundant prefix was the alternative, not the chosen rule.

Copilot recorded the decision in DD55, reconciled DD58/DD70/DD111 and
GUIDE, and added the P3 implementation bug
`reject-statusless-intrinsic-suppression`. R3 was the last policy
question in `q-suppression-reach-residuals`, so that record is now done
as a question. Its adjacency, operand-barrier and compound repairs
remain in their separate open implementation tasks.

The result-channel principle covers string and integer value intrinsics.
A capture's surrounding assignment, argument or interpolation position
does not provide another result; nor does a redirect wrapper. Existing
value-placement and redirect-syntax restrictions remain. This is not a
constant-success check or a claim that a pure operation cannot encounter
a fatal allocation fault.

There are two important preservation boundaries. `getenv` has a real
lookup status in addition to its value (DD101), unlike `toupper`.
Ordinary `- int_fn` and `- wait` discard the integer or pid itself,
without also capturing it; they remain legal. DD126's `waitstatus`
remains a separate child-result output. No ordinary integer result is
converted into a status, and no new intrinsic statement form is added.

Operand/body failures retain no-reach. DD130 leaves heap exhaustion and
other fatal faults fatal, and permits recoverable redirect handling only
where the handling form is otherwise legal. Neither failure class changes
the result kind. The earlier discarded-status-consumer rejection remains:
having a real status does not permit both discarding and testing it.

The historical noreach.12 used `$(- strlen $(/bin/false))` to exercise
an operand barrier. Its contract must be reviewed when the new rejection
lands: an earlier compile-time refusal cannot prove that the operand's
runtime failure would remain checked. The intrinsic and adjacency tasks
now coordinate that obligation, and the integer-capture task links the
shared classification boundary. Changed contracts require fresh ids;
the impact is not assumed to be limited to this one example.

The next discussion isolates control-transfer prefixes, including DD128's
`fail`. The old Q1 recommendation grouped `:` with them, but DD69 gives
it a status and DD130 makes `- : >optional.log` a meaningful recoverable
file-error discard. The question record now distinguishes those cases
and labels its older return/capture measurements as historical. No
control-transfer, no-op or discarded-test policy was selected here.
No new diagnostic code or implementation architecture was selected.
The historical native/C probes were not rerun. No compiler, runtime,
spec or build code changed; no new behavior probe, full sweep or coverage
run was made. Journal 577 remains the accepted implementation baseline.
