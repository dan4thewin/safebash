title: Explicit termination implementation
status: complete

DG's journal 575 decision is implemented by Copilot in session
ed4c7981-c5a0-4e82-9832-9557dc269030 on `exit-fail-return-kinds`.
`exit` selects EXIT for every integer; `fail` selects ERR, defaults to one,
and permits zero. Both backends preserve i64 operands and terminate the path.
Existing runtime progression, child ownership, cleanup and signal guards
are reused. Source and SSA EXIT-entry checks now include nonzero exits.
Operational exec/setup failures retain the abort ABI.

The new dual-mode `termination.sh` supplies 38 focused checks. Sixty-four
existing checks received fresh IDs: failure-intent exits became fail, and
seven fixtures renamed their now-reserved `fail` helper to `boom` without
weakening assertions. Independent reversions turned red three source-entry
checks, one SSA-entry check, and fifteen fail-routing checks per backend.
Linking the new compiler against the frozen old runtime turned four of
seven routing controls red in both backends. Native controls use a private
ld wrapper because the native CLI uses its build-time runtime path rather
than SASH_RT_LIB; the wrapper logs the actual archive selected by that env.

The original baseline is rowid 6822964, 11144 passing plus 13 skipped.
The first sweep, 6822965-6834197, correctly found the seven helper-name
collisions (14 mode failures); all other predictions matched. The next
attempt was interrupted by a host reboot and added no history.
The recovered ordinary `mk -p 1 gate` passed at rowids 6834198-6845430:
11220 passing, 13 skipped, zero failures, 14 new IDs, 14 retirements,
zero flips. Across both completed sweeps the unit adds 76 checks net.
All reversion stubs were restored; the frozen compiler/runtime hashes remain
unchanged. Evidence is in the assigned private
`exit-fail-return-kinds-unattended` directory (`recovered-gate-a.txt`,
`baseline/`, `revert-*.txt`, `migration-a.tsv`).

Unit B's return contracts and ceremony retirement follow separately.
