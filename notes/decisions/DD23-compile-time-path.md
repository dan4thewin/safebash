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

An absolute path skips the PATH search, but it is held to the same
standard — see the SB5095 addendum below.

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

## Clarification (DG, 2026-06-21)

- **No bare externals reach the back-end.** Every external is mapped to an
  absolute path; this resolution must run **ahead of the front-end checks**
  (validate/taint/type-check), not in the transform as it does today. See
  `advance-dd23-resolution`.
- **Taint source = `execve(2)`/`execveat(2)`.** Any captured output that
  passed through an exec is tainted (e.g. `printf`, `pwd` — real programs).
  In-process internal builtins (echo, getcwd, getpid, …) and inlined user
  functions are clean; `getenv`/`argv`/`arg` are tainted external input.
  See `tighten-cmdsub-taint-execve`.
- **--sysroot incl. squashfs** is part of resolution (build host default,
  or a target dir / squashfs image root).

## Addendum (2026-09-03) — relative external paths are rejected (SB5051)

DG hit two coupled gaps while running an arg-parser with
`PATH=.:$PATH ./sash 17` (a `sash-getopt` capture). `sash-getopt` resolved to
the **relative** path `./sash-getopt` (a `.`-in-PATH match), and:

1. The CST taint pass (`command_name_is_taint_source`) detected externals with
   `name.starts_with('/')` — **absolute only** — so the relative-path capture
   was **not** flagged. Only the fail-closed SSA taint backstop (basename-based)
   caught it, producing an "SSA taint backstop" reject where a clean **SB5020**
   should have fired. That divergence is the symptom; the root is that a
   relative external path exists at all, which DD#23 forbids.

**Rule (reaffirming DD#23's "absolute path only / no search ambiguity"):**

- **Resolution never yields a relative path.** `resolve_command_path` searches
  only **absolute** PATH entries; a relative dir (`.`, `..`, an empty entry =
  cwd) is skipped. A command found solely via a relative dir is unresolved →
  **SB5011**.
- **A source-written relative external head is a compile error — SB5051.** A
  literal command head that contains `/` but does not start with `/` (`./x`,
  `../x`, `a/b`) is rejected uniformly in every position (top-level, `$( )`,
  pipeline stage). An **absolute** `/x` path is unaffected; a bare unknown name
  is still "not found" (**SB5011**), which is distinct from a relative-path
  error.
- **CST taint hardening (defense-in-depth).** `command_name_is_taint_source`
  now uses `contains('/')` (not `starts_with('/')`), so a relative external
  would be tainted even if one ever slipped the rejection. The SSA backstop was
  already fail-closed (basename → tainted unless a known clean builtin/user fn).

Impl: `resolve.rs` (`resolve_command_path` dir-skip; `resolve_command` SB5051;
`is_valid_command` defers relative heads so `resolve` owns the diagnostic),
`validate.rs` (`command_name_is_taint_source` → `contains('/')`). Specs
`errors.sh` relpath.1–6 (×3 modes). SB5051 is a new code.

## Addendum (2026-10-09) — an absolute command head must be an executable
## file (SB5095)

DG, on reading the DD#126 §2 journal: *"the /nonexistent should be caught
at compile time, yes?"* — and then: *"reject any path that doesn't resolve
(or is not executable - directory, device, mode 444 ...); new code"*.

### The asymmetry

Three spellings of "this program is not here", and only two were caught:

| head | before |
|---|---|
| `nonexistent_binary_xyz` | **SB5011** at compile time |
| `./nonexistent_binary_xyz` | **SB5051** at compile time |
| `/nonexistent_binary_xyz` | **compiled**, then 127 at runtime |

The uncaught one is the spelling the author was most explicit about.
`is_resolvable_external` returns false for any name containing `/`, so an
absolute head was never looked at — even though `resolved_path_exists`
(sysroot- and squashfs-aware) sat beside it and was already the answer for
every bare name. `--manifest` went further and listed
`/nonexistent_binary_xyz` as a dependency: a requirement the compiler knew
could not be met, asserted as fact.

### The rule

A **literal absolute command head** must name something the resolution root
reports as a **regular file with at least one execute bit**. Anything else
is **SB5095**, and the diagnostic names which fault it is:

- `no such file`
- `it is a directory`
- `it is not a regular file` (device node, socket, FIFO)
- `mode 0444 grants no execute permission`
- `its symlink chain does not terminate`

"Command not found" would have been the wrong sentence for four of those
five — a reader told that about a mode-0644 file goes looking for the file.

The resolution root is the same one the PATH search uses: the build host, or
`--sysroot=<dir>`, or `--sysroot=<image.squashfs>`. Symlinks are followed
(`/bin/sh -> dash` is a program), in the image as well as on the host.

The rule applies uniformly in every position a command head appears —
top-level, pipeline stage, `$( )`, `&`, `-` — exactly as SB5051 does, and
to an `exec` target, where it matters most: a failed `exec` has already torn
down the process image that would have reported it.

### Scope boundaries

- **`[[ -x /path ]]` is untouched.** This DD names it as *the* runtime
  existence test replacing `command -v`; rejecting it would delete the
  escape the DD prescribes. SB5095 reads command heads, not test operands.
- **Globs are untouched.** A head containing `*`, `?`, `[` or `$` is not a
  literal path, and `validate`'s `is_valid_command` already exempts it. The
  only failure SB5095 cannot afford is a false reject.
- **Relative heads still get SB5051, bare names still get SB5011.** Three
  distinct facts, three codes (`errors.sh` cmdpath.15-16 pin the
  distinction).

### The PATH search got the same rule

`resolve_command_path` tested `is_file()`, so a *present but unrunnable*
candidate won the search: a mode-0444 `cptool` early in PATH shadowed the
real one later, and the failure arrived at `exec` time with the shadowing
path already baked into the binary. It now requires an executable, which is
the shell's own rule, and keeps looking otherwise (`cmdpath.17`).

### What this costs, and why it is still right

**The chain-alternatives example is withdrawn.** This DD used to offer

    /usr/bin/grep pattern file || /bin/grep pattern file

which only makes sense if an absolute path may be absent. As DG and I read
it, that example is at odds with this DD's own spine: a hand-written
two-entry fallback *is* the search ambiguity the security argument forbids.
The sanctioned escapes remain `--sysroot` (resolve against the target, not
the build host) and `/usr/bin/env tool` (delegate the search, explicitly).

**It tightens cross-compilation.** Compiling for a root you do not have now
requires `--sysroot` or `env`. That obligation was always implied — a bare
name has been resolved against the build host since DD#23 was written — but
an absolute head used to slip it.

**It costs a spec idiom, and that needed a replacement rather than a
shrug.** Roughly a dozen specs provoked the runtime's exec-ENOENT branch by
naming a path that was never there. Those programs no longer compile, and
the branch is still live — a program can be removed, replaced or unmounted
between build and run. `spec/common.sh` grows `run_sash_vanish`, which
stages a real program, lets the compiler see it, and deletes it before the
binary runs. That is the honest shape of the case: the spec *is* the program
that disappeared.

### Deferred

DG: *"maybe add a `--target-user` switch to do fine-grained access
checking"*. The check today is coarse — *some* user can execute this. Which
user is a question the compiler cannot answer without being told who the
target runs as. Filed as `notes/tasks/target-user-access-check.md`.

Impl: `lib.rs` (`FileKind`/`PathFacts`/`PathMiss`/`root_path_facts`, and the
squashfs listing moved from `unsquashfs -l` to `-ll` so it can report type
and mode at all), `resolve.rs` (`command_path_fault`,
`resolved_path_executable`, the SB5095 arm in `resolve_command`),
`validate.rs` (`check_exec_target`). Specs `errors.sh` cmdpath.1-17.
SB5095 is a new code.

## Addendum (2026-10-27) — resolution reaches every `$( … )` body (B37)

Resolution is a rewrite that the taint pass depends on: a capture is a
taint source when its head contains a `/`
(`validate::command_name_is_taint_source`), and a bare head is left clean
because it might be a user function. So a command that resolution never
reaches keeps its bare head and reads as clean.

`resolve.rs` walked statements with a private copy of the expression walk,
and the copy skipped three places that the checkers' walk
(`Statement::cmdsub_bodies`) visits: a parameter expansion's index,
`$(( … ))` leaves and `(( … ))` tests. A command there stayed bare for the
checkers. The transform resolves heads again as it lowers
(`transform/resolution.rs`), so the program still ran the real `cat`:
`b=${m[$(cat f)]:-d}` compiled and used the file's content unattested.
Other shapes were caught only by the SSA backstop, which reports no
position.

The rule: resolution walks the same `$( … )` bodies, in the same order, as
the checkers. It reaches them through `Statement::cmdsub_bodies_mut`, the
mutable twin of `cmdsub_bodies` in `ir.rs`, and `ir::cmdsub_twin_tests`
pins the two walks to identical results, so there is no private walk left
to drift. One visible consequence: when an indexed assignment's value and
index both name a missing absolute program, the value's SB5095 is now
reported first, as SB5011's already was.

Impl: `ir.rs` (`cmdsub_bodies_mut`, `own_expr_roots_mut`,
`push_cmdsub_bodies_mut`, `walk_arith_expansion_roots_mut`,
`walk_test_operands_mut`), `resolve.rs` (`resolve_stmt`). Specs `taint.sh`
capres.1-20. Journal 538.
