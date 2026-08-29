---
title: "De-collide spec chk ids (cross-file + within-file duplicates)"
status: done
priority: P2
kind: chore
depends: []
gates:
  - "no test_id is emitted by two spec files"
  - "no id has two executing `chk` lines in one file"
---

## Done (2026-08-29)

`chk` ids are a flat GLOBAL namespace (AGENTS.md). A duplicate id — across two
files, or two executing `chk` lines in one file — makes the later write silently
overwrite the earlier in `spec.db`, **masking failures**. This bit us once
(commands.sh 14.7 masked by wordsplit.sh 14.7; see
`bug-nested-bare-cmdsub-bless-taint`). Audited the whole suite and fixed every
collision. All renumbered tests are unchanged; only ids moved (plan counts are
preserved). native + c: 0 failing after the sweep; none of the de-collided
tests unmasked a real failure (the colliding pairs were all passing).

### Cross-file collisions fixed (renumbered the intruder to a free same-DD sub)
- arithmetic.sh ↔ types.sh `58.10-58.15` → types.sh moved to `58.30-58.35`
  (arithmetic.sh owns DD#58 arith-sexpr).
- convert.sh ↔ types.sh `30.7` → convert.sh moved to `30.30`.
- misc.sh ↔ wordsplit.sh `20.3-20.6` → misc.sh moved to `20.30-20.33`
  (wordsplit.sh owns DD#20).
- types.sh ↔ wordsplit.sh `62.11` → wordsplit.sh moved to `62.30`
  (types.sh owns DD#62).
- (earlier, commit de79db9) commands.sh ↔ wordsplit.sh `14.7/14.8` → commands.sh
  moved to `14.17/14.18`.

### Within-file duplicates fixed (types.sh had 2-3 blocks reusing ids)
- types.sh second block `30.8-30.11` (unset semantics) → `30.40-30.43`.
- types.sh late block `33.11,33.12,33.14-33.18` → `33.40-33.46`.
- errors.sh second `55.30` (open-failure-catchable) → `55.50`.

## Re-run the audit (keep it clean)

```python
import re, glob, collections
chk=re.compile(r'(?<!skip_)\bchk\s+([A-Za-z0-9][A-Za-z0-9._-]*)')
anyid=re.compile(r'\b(?:chk|skip_chk)\s+([A-Za-z0-9][A-Za-z0-9._-]*)')
xf=collections.defaultdict(lambda: collections.defaultdict(int))
pf=collections.defaultdict(lambda: collections.defaultdict(int))
for f in glob.glob('spec/*.sh'):
    if f.endswith(('common.sh','run.sh')): continue
    for line in open(f):
        if line.lstrip().startswith('#'): continue
        for m in anyid.finditer(line): xf[m.group(1)][f]+=1
        for m in chk.finditer(line): pf[f][m.group(1)]+=1
print('cross-file:', {i:list(d) for i,d in xf.items() if len(d)>1})
print('within-file dup chk:', [(f,i,c) for f in pf for i,c in pf[f].items() if c>1])
```

Both must be empty. (The many `chk N … else skip_chk N` mode-guard pairs are
fine — the audit counts only executing `chk` for within-file dups.)
