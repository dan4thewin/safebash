title: DD#53 — Env var isolation
status: approved

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
