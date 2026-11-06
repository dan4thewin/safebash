title: Left-to-right evaluation, by least surprise
status: complete

DG chose left-to-right evaluation, citing the principle of least surprise.
The rule covers both whole words and their component expressions: an
earlier read takes its value before a later capture runs, and later reads
see that capture's effects. Value-producing function calls follow the same
rule. This is not a return to subshell isolation.

For `str g=old; fn f { g=new; echo X }`, the command
`echo "$g" "$(f)" "$g"` must print `old X new`, not the current
`new X new`. Putting the expansions into one word does not create an
exception. Existing lazy operands, first-match dispatch and body/process
boundaries remain intact. DD55's first-expression `$?` rule follows the
same evaluation order.

Copilot recorded the contract in DD86/DD55, clarified DD66's already-shipped
lazy chain versus pending within-pattern ordering, and updated GUIDE.
`q-capture-hoisting-order` keeps its existing slug but is now an open bug
task with implementation gates. The findings entry remains unchecked.
Neither visiting a read early while loading its value late nor imposing
order in just one backend would satisfy the rule.

The related questions remain distinct: a capture body's permission to
inherit caller status, and the phase at which a pipeline stage installs
its redirects. Their task records now distinguish them from the settled
word-evaluation order.

The next decision presented is setup-failure suppression. DD128 already
settles heap exhaustion and arithmetic faults as fatal, so those are not
reopened. Three current native probes at `a7787b24` confirm the remaining
split: a suppressed job's injected fork failure and a suppressed stage's
redirect-open failure both continue silently, while the same redirected
statement aborts. The successful/injected first-clone controls and sources
are retained in session `files/setup-policy-20261106/`; C and the rest of
the historical matrix were not re-run. No setup-suppression ruling has
been made.

This is policy/backlog recording only. No compiler, runtime, spec or build
source changed, and no new spec-history rows were written. Implementation
remains queued; journal 577's accepted gate is unchanged.
