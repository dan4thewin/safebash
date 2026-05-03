title: GUIDE.md — DD#46 not reflected in taint source list
status: resolved
source: gpt-5.4
severity: accuracy

GUIDE.md Phase 3 Taint still lists positional params ($1-$N, $@) as
taint sources without the DD#46 distinction: function-local only in
sash-native mode, top-level references are compile errors. Update the
guide's taint source list.
