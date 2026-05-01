title: No decision on error handling model beyond - cmd
status: ready
source: claude-opus-4.6
severity: gap

DD#21 defines `- cmd`. DD#20 bakes in set -e semantics. But no
comprehensive decision covers: what happens when external commands
fail? How does set -e interact with pipelines, command substitution,
if conditions? Bash's set -e semantics are notoriously context-dependent.
