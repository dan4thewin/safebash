title: DD#23 vs GUIDE.md — PATH lookup and sash_setenv contradiction
status: resolved
source: gpt-5.4
severity: contradiction

DD#23 says PATH is compile-time only, scripts cannot modify PATH at
runtime. But GUIDE.md documents `sash_exec — PATH lookup` and
`sash_setenv` exists in the runtime. If PATH is compile-time, the
guide is describing capabilities that should be absent or constrained.
Environment mutation semantics beyond PATH are unspecified.
