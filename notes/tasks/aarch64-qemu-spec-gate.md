title: "Gate AArch64 Linux native and C output under QEMU"
status: open
priority: P1
kind: work
depends: [native-cross-codegen-linking]
gates:
  - "A target gate compiles through the real host CLI, executes AArch64 native/C outputs under QEMU and fails on mismatches."
  - "Startup/trace, regex recovery, process/descriptor/handler behavior and actual guest fault paths are asserted."
  - "Target result identity, baseline bootstrap, counts, skips and regression comparisons cannot overwrite or impersonate host evidence."

# QEMU-backed AArch64 Linux spec gate

Complete the first x86_64 Linux -> AArch64 Linux milestone from the
[roadmap](cross-compile-and-setjmp-on-mac.md). The preceding codegen/link
task already executes its first target programs; do not delay all QEMU
testing until this larger harness adaptation is finished.

Extend the existing `spec/common.sh`, `spec/run.sh` and gate/build entry
points rather than create a competing end-to-end suite. Currently the
harness chooses one runtime, calls host `gcc`, directly runs outputs and
records backend-only IDs in root `spec.db`. Account for every direct
compile/link/execute site, not just `run_sash`'s common path.

Keep the compiler and its helpers on the intended compiler host; select
target flags, matching target C compiler/runtime and QEMU execution for
generated programs. Use target external commands and prove nested exec
does not accidentally run host `/bin` programs. `--sysroot` is a compile-time
resolution aid, not runtime isolation.

Required coverage includes argv/environment/startup, arithmetic and
collections, captures/readers, fork/exec/pipelines/wait, redirects,
EXIT/ERR/signals, and regex valid/invalid/repeated recovery. In particular,
inherit the unmet ARM execution gate from
[the completed startup repair](bug-aarch64-trace-init-missing.md):
compile with `--trace`, prove enabled branch output, and prove disabled
tracing stays silent. Do not replace it with a source-text assertion.

Use the [QEMU environment](aarch64-toolchain-qemu.md) for guest syscall/
fault evidence. Host strace around a user-mode emulator measures emulator
syscalls, not the guest injection point. Recheck ARM syscall names/numbers
and restart counts; apply limits/injection to the program, not the compiler
or an unrelated emulator allocation. Run kernel-dependent controls in a
valid guest environment instead of silently skipping them.

Choose explicit host/target/backend result identity or isolated target
history before the first sweep. Keep ordinary x86 results and managed
status output intact. An empty target history cannot satisfy the current
gate's first-status/current-status liveness assumption
(`contrib/gate.sh:174-182`); supply an honest bootstrap and can-fail control,
not fabricated prior ARM results or a disabled comparison.

Predict actual per-target totals and every justified skip. Preserve global
spec IDs, plan checks, retirement caps, previous-row comparisons and failure
exit behavior. Prove the gate fails for a wrong-architecture executable, a
missing runner/tool, a failed program and an omitted check. Record exact
compiler/runtime/environment identities and serialized, bounded commands.
Do not call an unrun or unsupported cohort passing.
