title: No decision on dangerous external program arguments
status: ready
source: gemini-pro, gpt-5.4
severity: gap

SUBSET.md explicitly says safebash does not inspect arguments to
xargs, find -exec, parallel, etc. GPT-5.4: "the docs want credit
for safety while disclaiming one of the biggest remaining attack
surfaces." A decision should state the boundary of responsibility
and why this limitation doesn't invalidate the safety claims.
