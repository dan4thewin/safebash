title: DD#49 — syntactic bless check bypassable with tautology
status: resolved
source: gemini-pro
severity: gap

DD#49 requires the test expression to contain `$VAR`. But
`bless x [[ "$x" == "$x" ]]` satisfies the syntactic check while
being always true — performs no validation. Consider whether semantic
analysis of test utility is needed, or whether this is an acceptable
residual risk (the programmer wrote deliberately useless validation).
