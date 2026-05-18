title: getenv/argv/argc builtins
status: done
severity: feature
gates: [42.1, 42.3, 42.5, 42.6]
---

DD#42: $(getenv VAR) reads env (tainted), $(argv N) reads positional
(tainted), $(argc) returns count. Out-of-range argv aborts (like
integer overflow). All 6 specs passing.
