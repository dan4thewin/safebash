title: No decision on shell builtins that mutate process state
status: ready
source: gpt-5.4
severity: gap

SUBSET.md lists builtins including trap, cd, unset, shift, command.
One-off decisions reject export, source, subshells. But no coherent
policy for which stateful builtins are part of the trusted semantic
core and why. Builtins are where shells hide weird ambient state.
