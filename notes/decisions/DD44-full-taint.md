title: DD#44 — Full taint quarantine at all use sites
status: implemented

Tainted data cannot be used at ALL use sites until blessed — not just
"dangerous" ones. `echo $var` piped to `xargs rm -rf` — the echo is
dangerous. No propagation, only per-definition classification (amendment 2).
