title: "Decide the existing C differential backend's link isolation"
status: open
priority: P3
kind: decision
depends: []
gates:
  - "The intended libc/default-library boundary and any need for pthread linkage are measured and explicitly decided."
  - "The decision distinguishes the existing C oracle from native no-libc output and the separate C++ transpiler."

# Existing C differential backend link policy

Split from the [native cross-compilation coordinator](cross-compile-and-setjmp-on-mac.md)
so its pre-existing policy question does not block first AArch64 delivery.
DD129 settles the native no-libc runtime, not this C-link-command choice.

At `4a3286cb`, `spec/common.sh:189-190` invokes `gcc -nostartfiles -static
-fno-stack-protector` with `libsash_rt.a` and `-lpthread`. Investigate the
actual resolved symbols and default libraries on the supported targets.
Present whether the oracle should use `-nostdlib` to expose accidental libc
references like native linking, and whether explicit pthread linkage is
needed, with consequences for generated C and toolchain-provided helpers.

No alternative is selected by filing this task. Target-aware C compiler/
runtime selection belongs to the ARM spec gate and need not await a broader
link-policy change. Conversely, target failures must not be hidden with
additional libraries merely to get a green ARM run.
