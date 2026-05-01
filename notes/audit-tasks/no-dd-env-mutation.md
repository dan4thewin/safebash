title: No decision on environment mutation semantics
status: ready
source: gpt-5.4
severity: gap

sash_setenv exists in runtime. DD#23 bans PATH mutation. But no
decision covers: are non-PATH env mutations allowed before exec?
Is `FOO=bar cmd` the only allowed outbound env mechanism (as DD#01-09
item 4 implies)? What about runtime unset of env-backed names?
