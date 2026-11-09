title: DD#53 — Env var isolation
status: implemented

Sash variables and environment variables are completely separate
namespaces. There is no implicit mirroring in either direction.

## Reading env vars

The only way to read an environment variable is `$(getenv NAME)`.
The result is tainted (DD#24) and must be blessed before use.

`$HOME` in sash source refers to a sash variable named `HOME`. It
does NOT implicitly read the `HOME` environment variable. If no sash
variable named `HOME` has been assigned, `$HOME` is empty.

Scripts that need env values must be explicit:

```bash
home=$(getenv HOME)
bless home [[ -d $home ]]
```

Auto-bless patterns in `known_externals.rs` provide conservative
validation for common env vars, but the `$(getenv NAME)` call is
still required.

## Writing env vars

There is no mechanism to push sash variable changes back to the
environment. No `export`, no `declare -x`, no `sash_setenv`.

The `export` keyword is a compile error (SB5001, DD#45 amendment).

## Per-command env vars

Delegate to `/usr/bin/env`:

```bash
/usr/bin/env FOO=bar /usr/bin/grep pattern file
```

Sash has no prefix-assignment syntax for env vars. The coercion from
bash `FOO=bar cmd` prepends `/usr/bin/env`. This keeps all env var
manipulation outside the compiler and runtime.

DD71's 2026-11-09 boundary rule also rejects `FOO=bar cmd` as an
implicit sequence of a sash binding and a command. Write `FOO=bar; cmd`
for that sequence, or use a newline; neither form sets the command's
environment. One or several bindings follow the same rule, with or
without a declaration keyword. Enforcement is pending in
`q-statement-form-residuals`; `/usr/bin/env` remains the explicit
environment mechanism.

## Rationale

The bash model — where shell variables and env vars share a namespace,
with `export` flagging which ones propagate — is a source of confusion
and security bugs. Sash makes the boundary explicit:

- Env vars are external input → tainted by default
- Sash vars are program state → clean unless assigned from tainted source
- The boundary crossing is always visible in source (`getenv` + `bless`)
- Auditors can grep for `getenv` to find every env-var dependency

## Supersedes

The implicit getenv fallback from experiment 027.

## See also

- DD#24 — Taint quarantine and bless (getenv as taint source)
- DD#42 — getenv and argv as builtins (explicit-origin forms)
- DD#45 — Export rejection amendment (SB5001)

## Addendum 2026-09-12 — getenv reports a lookup status

`$(getenv NAME)` now carries a **lookup status**: unset is a failure, set
(including set-to-empty) is a success. This makes unset and `NAME=""`
distinguishable, which they were not before. Uncaught is allowed; the value is
`""` either way and taint still gates every use. See
[DD#101](DD101-getenv-lookup-status.md).

Note the examples above use `bless`, which DD#93 renamed to **`attest`**.

## Addendum 2026-09-25 — `cd` requires its directory (SB4066)

Env isolation has a consequence for `cd` that was never written down: with no
implicit `$HOME`, a bare `cd` has nowhere to go. Both backends answered that
silently — `$? = 0` and `test_result = 1`, i.e. **success**, with the working
directory unchanged, where bash goes to `$HOME`. A second argument was dropped
just as quietly (`cd /tmp /var` chdir'd to `/tmp`). Neither form had a spec.

`cd` now takes **exactly one argument**; zero or more than one is **SB4066**.
Unlike `close`'s fd (SB4052), the argument need **not** be a literal — a path
is not a runtime-chosen descriptor, so `cd $dir` stays legal. A script that
wants the home directory must say so under the usual rules:

```bash
h=$(getenv HOME)
attest h [[ -d $h ]]
cd $h
```

This closes the last of the "defensive" silent-no-op branches in the two
filesystem/fd builtins: the guards in `handle_cd`, `handle_close`, `emit_cd_c`
and `emit_close_builtin` are now assertions naming the validator code that
makes them unreachable.

Impl: `validate.rs` (`check_cd_arg` → SB4066, dispatched from
`check_command_head` beside `check_close_arg`), `codegen/inst_handlers.rs`,
`emit_c.rs`. Specs `spec/errors.sh` cd.1–cd.3 (both backends). SB4066 is a new
code.
