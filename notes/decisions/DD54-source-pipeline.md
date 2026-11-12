title: "DD#54 — Source replacement: sash-expand pipeline"
status: implemented

1. Load configuration as data through an explicit external-tool pipeline,
   without executing it or round-tripping it through the environment.
2. `sash-expand` emits validated-format key/value text; read, split and construct
   collections using DD99's reader and array-to-map contracts.
3. Kernel-read configuration remains tainted and requires the ordinary
   adjacent attest/trust discipline.

---

`. ./file` and `source ./file` are rejected (SB4002). The runtime
config-loading use case is served by an explicit pipeline:

<del>

```bash
map config
sash-expand ./vars.conf | mapfile -m config
bless config [[ ... ]]
```

</del>

DD99 replaces this reader surface; DD93 renames the validation operation.

## Components

**`sash-expand`** — standalone external binary. Reads a file, evaluates
the safe expansion subset (simple expansion, defaults, alternates,
prefix/suffix stripping), outputs KEY=VALUE lines to stdout. Dies on
non-conforming input. This is the same tool from DD#01-09 Decision 1,
unchanged in function — only its integration point changes.

~~**`mapfile -m`** — new switch. Reads KEY=VALUE lines from stdin into
a map variable. Each line is split on the first `=`: key is the map
key, remainder is the value. Blank lines and `#` comment lines are
skipped (matching sash-expand output conventions).~~

~~**`bless`** — standard taint gate (DD#24). `sash-expand` is an external
command, so its output is tainted. The map must be blessed before use.~~
DD127 classifies the kernel read, and DD93/DD128 govern attest and resolution.

## Rationale

The prior design (DD#01-09 Decision 1) coerced `. ./vars` into a call
to `sash_source_kvp`, which parsed KEY=VALUE lines and pushed them into
the process environment via `sash_setenv`. Codegen then re-read them
via `sash_getenv` into sash variables. This:

- Depended on implicit env-var mirroring (violates DD#53)
- Bypassed taint/bless (env vars silently became sash vars)
- Required special codegen handling (post-source re-read loop)
- Pretended `. ./vars` still worked when the semantics were
  fundamentally different

The pipeline approach is explicit, auditable, and composes from
existing primitives (external command, pipe, ~~mapfile, bless~~).

## Supersedes

- DD#01-09 item 2: "Reject `source` / `.` (SB4002) — safe loader
  deferred" → SB4002 is now a hard rejection, not a coercion marker
- DD#01-09 Decision 1: "source / . — compiled away, not rejected"
  → reversed. Source is rejected. The pipeline is the replacement.
- DD#01-09 Decision 6: source and . treated identically → unchanged,
  both rejected

## See also

- DD#53 — Env var isolation (motivates removing the env roundtrip)
- DD#24 — Taint quarantine (sash-expand output is tainted)
- DD#42 — getenv/argv builtins (getenv remains for individual env
  var reads; the KVP loader no longer uses it internally)
