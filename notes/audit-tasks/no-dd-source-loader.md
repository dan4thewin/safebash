title: No decision on source/. safe loader design
status: resolved
source: claude-opus-4.6
severity: gap

DD#01-09 item 2 says "safe loader deferred." SB4002 rejects `source`.
No decision describes what the safe loader would look like. For a
compiler whose goal is replacing bash scripts, inability to compose
multiple files is a significant limitation.
Resolution: DD#53 (env isolation) and DD#54 (source pipeline).
