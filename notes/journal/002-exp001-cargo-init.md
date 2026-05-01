# Experiment 001: cargo init — proving the Rust toolchain works

**Goal:** Create a Rust binary project named `sash`, compile and test it.

**Actions:**
- `cargo init --name sash` in `exp/001-cargo-init/`
- Added a trivial test (`assert_eq!(2+2, 4)`)
- `cargo build` succeeded, `cargo test` passed, binary prints
  `sash: safebash compiler`

**Results:**
- Rust 1.93.0 (cargo 1.93.0) on ROVE
- Edition 2024 selected automatically
- Build time: ~2s (clean)

**Conclusion:** Toolchain works. Binary name is `sash`.

---

