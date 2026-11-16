title: Brush suite re-run and triage
status: superseded
depends: []
gates:
  - commit

## Superseded; record reconciled 2026-11-16

DG retired the external suites; [journal 621](../journal/621-retire-external-suites.md)
records removal and the `archive/legacy-suites` retrieval tag. The old Brush
runner/pass-rate refresh is no longer current work. Possible future reuse
of upstream cases for converter testing is not a completion claim for this
rerun.

## Historical task

Re-run the 622 Brush test cases and update pass rates. Last full
run was 2026-04-25 (160 PASS, 59 FAIL_OUTPUT, 75 FAIL_COMPILE).
Triage in `audit/run-2/BRUSH_TRIAGE.md`.
