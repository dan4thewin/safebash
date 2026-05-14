title: DD#23 — squashfs sysroot for verified path resolution
status: done
severity: feature
depends: [dd23-sysroot]
gates: []
---

`sash --sysroot=image.squashfs script.sh` resolves commands against
a squashfs image without mounting it.

Read the squashfs directory structure to verify binary existence.
Combined with dm-verity, this guarantees the compiled binary's
external dependencies match the verified filesystem exactly.

Requires a squashfs reader (unsquashfs -l or a Rust crate).
