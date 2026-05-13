title: DD#23 — External Command Resolution
status: partial

Sash does not walk PATH at runtime. External commands are resolved to
absolute paths at compile time.

## Compile-time resolution

The compiler resolves bare command names to absolute paths:

    grep → /usr/bin/grep

Resolution searches the build host or a target sysroot:

    sash compile script.sh                          # build host
    sash compile --sysroot=/mnt/target script.sh    # target root
    sash compile --sysroot=image.squashfs script.sh # squashfs image

Unresolved commands are fatal compile errors.

## Absolute paths in source

Sash-native scripts use absolute paths directly:

    /usr/bin/grep pattern file

Or chain alternatives:

    /usr/bin/grep pattern file || /bin/grep pattern file

## Existence testing

`[[ -x /path/to/binary ]]` replaces bash's `command -v`. No
`command` builtin in sash-native — it's a coercion target only.

## Coercion from bash

    # bash:
    if command -v docker >/dev/null; then docker build .; fi

    # sash (coerced):
    if [[ -x /usr/bin/docker ]]; then /usr/bin/docker build .; fi

The path comes from compile-time resolution. `command -v` is
eliminated; `[[ -x ]]` tests runtime existence.

## The env escape hatch

When a command genuinely can't be resolved at compile time (not on
the build host, runtime-only tool), use `env`:

    /usr/bin/env some-tool args

`env` is itself resolved at compile time. It performs PATH lookup at
runtime. The delegation is explicit, visible, auditable. Sash never
walks PATH — it delegates to a program whose job is to walk PATH.

## Manifest

In resolve mode, the compiler emits a dependency manifest listing
every external binary the compiled script requires.

## Security argument

When the sash binary runs atop a dm-verity squashfs, the absolute
paths in the binary + the hash tree guarantee exactly which code
executes. No PATH injection, no symlink tricks, no search ambiguity.

## Runtime

`sash_exec` takes an absolute path only. If the argument contains no
slash, that's a compiler bug — the runtime should not attempt PATH
search.

## See also

- DD#53 — Env var isolation (PATH not readable unless via getenv+bless)
- DD#45 — Export rejected (can't modify inherited PATH)
