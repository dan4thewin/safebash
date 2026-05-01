title: No decision on signal/trap interaction with taint model
status: ready
source: claude-opus-4.6
severity: gap

`trap` is accepted (SUBSET.md builtins) but no DD covers how signals
interact with the safety model. Can a signal handler read tainted
data? Call external commands? The taint pass presumably walks trap
bodies, but there's no decision documenting intended semantics.
