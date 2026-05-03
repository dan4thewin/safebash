title: DD#50 vs SUBSET.md — "no propagation" claim tension
status: resolved
source: gemini-pro
severity: contradiction

SUBSET.md says "There is no propagation." DD#50 (function free-read
tracking) checks whether globals read by a function are tainted at
call sites — a form of inter-procedural analysis. The "no propagation"
claim is about the value model (tainted values can't be copied), not
about analysis scope. Clarify the language so "no propagation" isn't
misread as "no inter-procedural analysis."
