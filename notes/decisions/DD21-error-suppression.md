title: DD#21 — Error Suppression with - Prefix
status: designed

`- cd /tmp` suppresses cd failure (equivalent to `cd /tmp || true`).
Syntactic sugar for the `|| true` pattern.
