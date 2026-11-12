title: DD#109 — Every native compile embeds its IR and manifest
status: implemented

1. Every native compile carries source, inert IR and dependency manifest in
   the output artifact; extraction returns their original text.
2. Emitted and embedded IR/manifest use the same representation. Extraction
   defaults to source and does not translate IR schema versions.
3. An absent source or manifest section is empty only in an identified sash
   output. Missing IR, unreadable data and non-UTF-8 data are errors.
4. Terminal stdout emission modes remain separate from native binary creation.

---

Every native compile now embeds three artifacts in the linked ELF as note
sections: the source (`.sash_source`, DD#45), the IR capture (`.sash_ir`,
DD#104) and the manifest (`.sash_manifest`). They are retrieved with
`sash --extract <binary> [source|ir|manifest]`.

DG's call. The flags that used to be the *only* way to get an IR capture or a
manifest keep working unchanged.

## Why always-on

`--emit-ir` and `--manifest` were terminal stdout modes: they printed and
exited without producing a binary. So the IR emitter ran only when someone
passed the flag, and in practice that was four spec tests over three tiny
programs.

That matters more than convenience, because `emit_ir.rs` is **deliberately
exhaustive — no wildcard arms** (DD#104). Adding an IR variant is a compile
error there rather than a silently omitted node. But an emitter that is only
*run* on `echo hello` gets no benefit from being total: the exhaustiveness
catches a missing `match` arm at build time, while everything downstream of it
— escaping, nesting, node shape — is only checked on the paths someone
actually emitted.

Embedding on every compile closes that gap by construction. Every program the
spec suite compiles now renders its whole IR, so a variant that fails to
render is a failure of *every* build rather than a latent gap in a
rarely-taken flag.

The measurement that prompted it: `emit_ir.rs` was the single largest coverage
hole in the compiler at **30.8% regions / 33.7% lines, 409 missed lines and 35
never-executed functions** — every one of them an IR node emitter that no spec
ever reached.

## Why ELF sections rather than sidecar files

DD#45 already embeds the source this way, so the pattern and the `--extract`
retrieval path existed. Sections were chosen over `prog.ir` / `prog.manifest`
sidecars because:

- the artifacts travel with the binary and **cannot desync** from it;
- the output of a compile stays one file — 4548 spec compiles do not start
  littering two extra files each, and no spec that inspects a directory
  changes behaviour;
- note sections survive `ld --gc-sections --strip-debug`, which is why
  `.sash_source` has been readable since DD#45.

The cost is binary size: the IR is roughly 8% of a small binary (2680 B of IR
against a 35440 B binary for a 14-line program). Accepted.

## The empty manifest is empty, not missing

A **zero-length section does not survive into the linked ELF**. A program with
no external dependencies therefore has no `.sash_manifest` at all — and that
is the common case, since an all-builtin program depends on nothing.

Reading an absent `.sash_manifest` as an error would make the common case look
broken, so an absent manifest reads back as an empty one. An empty-source
program also loses its `.sash_source`, but its IR is always nonempty. Either
`.sash_source` or `.sash_ir` therefore identifies a sash output whose absent
source/manifest is empty; the source marker also preserves older outputs.
An ordinary ELF still errors. `--extract /bin/true manifest` fails, as it
should, and a missing IR is never treated as empty.

This fallback applies only to an **absent** section, not a failed read. A
present but unreadable or non-UTF-8 artifact reports its own error rather
than returning an empty string or the placeholder `<binary data>`.

The codegen artifact bundle requires all three texts, including legitimate
empty bodies. The obsolete optional fields had no production omission
caller and contradicted the always-on invariant.

## What did not change

- `--emit-ir`, `--manifest` and `--emit-c` remain terminal stdout modes that
  skip the binary. Specs use that form and it is the pipeline-friendly one.
- `--extract <binary>` with no section argument still prints the source, so
  the DD#45 spelling keeps its original meaning.
- `--emit-c` is out of scope: it writes C to stdout and produces no ELF, so
  there is nowhere to embed. Native compiles only, `-c` included.

The embedded artifacts are byte-identical to what the flags print — the
manifest section is built by the same one-path-per-line rule as
`emit_manifest`, and specs 109.2/109.3 assert the equality directly, so the
section and the flag cannot drift apart.

DD#128's handler split advances both emitted and embedded IR to schema 2
(DD#104), including programs without handlers. Extraction returns that
capture unchanged; it does not translate back to schema 1.

## Specs

The DD109 cohort in `spec/cli.sh` checks that IR round-trips out of a binary; IR and manifest
sections equal their flag output byte-for-byte; an empty manifest extracts as
empty at rc 0; the DD#45 no-argument spelling is preserved; an unknown section
name is rejected; a non-sash binary still errors. Empty-source source and
manifest extraction succeed, and all three artifact readers reject present
but unreadable/non-UTF-8 section data. The two version assertions migrate
from `109.1`/`.9` to `h128.109.1`/`.9` under DD128, retaining the nonempty
structure and byte-equality checks while requiring schema 2.
