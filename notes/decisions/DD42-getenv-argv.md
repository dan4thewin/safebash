title: DD#42 — getenv and argv as builtins
status: superseded
superseded-by: DD60

1. `$(getenv NAME)` is the explicit environment read and takes exactly one
   argument; its string result is tainted.
2. The lookup succeeds for a present name, including an empty value, and
   fails for an absent name. Absent lookup yields empty text without an
   implicit abort; its status can be handled through DD101's capture condition.

---

**The argv/argc half of this DD is superseded by [DD#60](DD60-argv-trio.md)**
(C-semantics `argv`/`arg`/`argc` trio). `$(getenv VAR)` is unchanged and
remains the canonical env-var read. The historical argv/argc text below is
retained for context; for current behavior see DD#60.

`$(getenv VAR)` reads an environment variable. ~~`$(argv N)` reads
positional parameter N (1-based). `$(argc)` returns the count of
positional params (equivalent to `$#`).~~

These are the explicit-origin forms for external data. Environment
variables accessed via getenv are tainted ~~(auto-blessed if in
known_externals)~~. Positional params via argv are tainted.
Converter-generated attests are not implicit compiler validation (DD57/DD93).

~~All three always refer to the **program's** arguments, never a
function's.~~ Functions receive their args via `$1..$N`.

~~Out-of-range `$(argv N)` aborts with a clear error message, consistent
with integer overflow behavior (DD#26). There is no silent empty return.~~

~~Codegen calls `sash_argv_get` in the runtime, which does bounds
checking and panics on violation.~~

## Addendum 2026-09-12 — `$(getenv NAME)` also reports a lookup status

Superseded in one respect by **DD#101**: `$(getenv NAME)` now sets an exit
status — 0 when the name is present (*including* when it is set to the empty
string), nonzero when it is unset — so

    home=$(getenv HOME) || home=/root

is a legal capture-condition. The value semantics above are unchanged: the
result is still a tainted string, still `""` when the name is absent, and
`getenv` still takes exactly one argument (DD#101 deliberately did **not**
adopt the `$(arg N DEFAULT)` shape).

Note for readers arriving from the abort discussion: this file's "There is no
silent empty return" is scoped to `$(argv N)` **out-of-range bounds**, the
sentence it follows. It is not a general principle about intrinsic results and
it says nothing about environment lookups — an earlier revision of
`notes/tasks/getenv-presence-handling.md` misread it that way.

## See also

- DD#24 — Taint quarantine and bless (getenv results are tainted)
- DD#26 — Integer overflow traps (same abort philosophy)
- DD#53 — Env var isolation (getenv is the only way to read env vars)
- DD#101 — `$(getenv NAME)` lookup status
