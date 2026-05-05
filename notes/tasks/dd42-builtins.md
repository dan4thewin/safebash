title: getenv/argv/argc builtins
status: ready
severity: feature
gates: [42.1, 42.3, 42.5, 42.6]
---

DD#42: $(getenv VAR) reads env (tainted), $(argv N) reads positional
(tainted), $(argc) returns count, out-of-range argv returns empty with $?=1.
42.2 and 42.4 (taint) are gated by taint tasks.
