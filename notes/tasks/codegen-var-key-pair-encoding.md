---
title: "codegen var map keys a (name, version) pair into a String, then parses it back"
status: open
priority: P3
kind: cleanup
depends: []
gates:
  - "no codegen helper re-derives a base name by parsing an SSA key suffix"
  - "sweep unchanged (5583, 0 failing) — this is behaviour-preserving"
---

# The var map encodes a pair as a string

`HashMap<String, VarSlot>` in codegen is keyed by `VarRef::display()` —
`format!("{}_{}", name, version)` over a `u32`. The name and version are
separately available at almost every insert site, and the string is taken
apart again wherever a base name is needed.

One of the two decoders was pure waste and is already gone (journal 363):
`base_var_name(&target.display())` in `inst_handlers.rs` was an identity
round trip recovering `target.name`, deleted in favour of `&target.name`.

**What remains** is `parse_var_base_version` (`codegen/redirects.rs:344`),
called by `find_highest_version`, which iterates map *keys* — there the
pair really has been flattened and has to be recovered.

Its three branches are permanently cold: `rfind('_')` never returns `None`,
the suffix is never empty, the suffix is always all-digits. Every key
producer is versioned (audited: `VarRef::display()`, `dispatch.rs`
466/489/491/525/533, `setup.rs:1391`). The fallback is defensive against a
key shape nothing constructs.

These are **not** ICE guards. On an unparseable key the fallback returns
`(key, 0)` — a silently-wrong answer, not a loud failure — so the usual
"keep the guard, it prevents a regression" argument does not apply.

## Options for DG

- **(a) Leave it.** Three permanently-cold branches, documented as such.
  Zero risk, and the 100% target never reaches this file.
- **(b) Key the map on `(String, u32)`**, or carry name+version in
  `VarSlot`. Removes the parse rather than testing it, so the cold
  branches cease to exist and the fragility goes with them. Touches every
  `vars` site across `setup.rs`, `dispatch.rs`, `values.rs`,
  `inst_handlers.rs`, `redirects.rs` — behaviour-preserving but wide.
- **(c) Delete the fallback**, panicking on an unparseable key. Reaches
  100% cheaply but converts silent degradation into an ICE on a path no
  input can reach — trading a dead branch for a new failure mode.

Recommendation: **(b)** when the codegen var map is next touched for
another reason; **(a)** until then. Not worth opening on its own account.

Note the doc comments on both helpers claimed keys could be bare `name`
for version 0. They cannot — v0 is `name_0`. That falsehood is what made a
collision bug look plausible and cost a round of investigation. Corrected
in the surviving comment.
