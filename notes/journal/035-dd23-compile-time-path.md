# Design Decision #23: PATH is compile-time only

**Date:** 2025-04-16

**Principle:** PATH is not a runtime-mutable variable. Scripts cannot
`export PATH=...` or `PATH=$PATH:/extra`. The compiler controls how
external commands are found.

### Two modes

**Mode 1: Compile-time PATH, runtime search** (default)

PATH is baked into the binary as a constant. At runtime, `sash_exec`
walks the compiled-in PATH to find binaries. Good for general-purpose
scripts (kusto, dev tooling) where the target machine is similar to
the build machine.

```
safebash compile script.sh                    # uses build machine's PATH
safebash compile --path=/usr/bin:/bin script.sh  # explicit PATH
```

**Mode 2: Compile-time resolution, absolute paths** (`--resolve`)

Every external command is resolved to an absolute path at compile time.
No PATH search at runtime — the binary calls `execve("/usr/bin/mktemp", ...)`
directly. Any unresolved command is a fatal compile error.

```
safebash compile --resolve script.sh
safebash compile --resolve --sysroot=/mnt/target script.sh
safebash compile --resolve --sysroot=image.squashfs script.sh
```

The `--sysroot` flag (same concept as GCC) points the compiler at a target
root filesystem. Commands are searched relative to sysroot but paths in the
binary are absolute relative to `/` on the target.

**Manifest:** In `--resolve` mode, the compiler emits a dependency manifest
listing every external binary the compiled script requires. Useful for
portable services — you know exactly what host binaries the service needs.

### Escape hatch

`env PATH=/special/dir command` is allowed in both modes. It runs a single
child process with a different PATH without affecting the compiled binary's
own resolution. This is the mechanism for commands that genuinely need a
different search path at runtime.

### Security argument

- Mode 1: No PATH injection at runtime — attacker can't prepend a directory
- Mode 2: No PATH at all — binary has absolute paths, zero search surface
- Both: `PATH=... command` in the script is a compile error
- `env` escape hatch is explicit, visible, auditable

### Use cases

- **kusto:** Mode 1 — general dev tooling, machines are similar
- **soc-os-update-flash.sh:** Mode 2 with `--sysroot` — systemd portable
  service atop a known base image, exact binary layout known
- **unittest:** Mode 1 — runs on the build machine

