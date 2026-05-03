title: No decision on exec without arguments (fd manipulation)
status: resolved
source: claude-opus-4.6
severity: gap

`exec` with a command replaces the process (exp 042). But `exec`
without arguments in bash modifies file descriptors (`exec 3>file`).
No decision covers whether this is supported or rejected.
