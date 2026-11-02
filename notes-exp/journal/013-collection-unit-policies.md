# Remaining policies for the next collection unit

DG asked for the remaining decisions, clarifying that the scope was only
the next unit of work. The AI presented three choices together.
DG selected rejection of negative array indices, population length, and
live sorted traversal.
[Experiment DD#7](../decisions/DD07-collection-indices-length-traversal.md)
records those choices.

Live traversal advances to the next greater currently present key, sees
later insertions and updates, skips deleted cells, and remains safe when
the body deletes the current cell or mutates through an alias.
It can keep extending when the body keeps inserting later keys.
Explicit snapshots and already-prepared argument vectors remain copies.

The earlier [DD#6](../decisions/DD06-missing-cell-reads.md) still governs
missing valid cells: typed zero without insertion. It does not make
negative indices valid. Stored empty/zero cells count toward length.

The AI updated the decision links, status, and runtime roadmap.
These decisions unblock the proposed collection/argument unit; they do
not settle unrelated text, arithmetic, regex, or capture questions.
No implementation began, no native code or producer changed, and the
protected producer draft remains uncommitted.
