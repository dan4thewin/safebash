title: Fuzz target for validator
status: ready
depends: []
gates:
  - tests
  - commit

No `cargo-fuzz` or property-test harness exists. Needed to find
validator bypasses the adversarial audit didn't catch.
