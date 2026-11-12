title: DD#23 — --sysroot flag for cross-compilation path resolution
status: done
severity: feature
depends: []
gates: []

`sash --sysroot=/mnt/target script.sh` resolves bare command
names against the target filesystem instead of the build host.

Each PATH component gets the sysroot prepended:
  /usr/bin/grep → /mnt/target/usr/bin/grep

Use case: compiling sash scripts for a different root filesystem
(embedded, container, chroot).
