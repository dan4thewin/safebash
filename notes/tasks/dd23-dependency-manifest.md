title: DD#23 — dependency manifest for resolved external commands
status: done
severity: feature
depends: []
gates: []
---

After compile-time path resolution, emit a manifest listing every
external binary the compiled script depends on:

  /usr/bin/grep
  /usr/bin/wc
  /bin/cat

Output via `sash --manifest script.sh` or embedded in the ELF
alongside .sash_source. Useful for container minimization and
deployment verification.
