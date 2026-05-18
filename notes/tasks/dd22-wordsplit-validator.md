title: Word splitting validator — reject IFS and for-in $var
status: done
severity: correctness
gates: [22.2, 22.3, 28.1]
---

Validator rules for DD#22/DD#28:
- IFS=... assignment SHALL be a compile error (22.2)
- for x in $var (unquoted expansion in loop header) SHALL be a compile error (22.3, 28.1)
