RT=sash-rt
SASH=sash
CONVERT=sash-convert
CSRC=`ls sash-rt/csrc/*.c sash-rt/csrc/*.S sash-rt/csrc/*.h | tr '\n' ' '`

build:V: $RT/target/release/libsash_rt.a target/release/sash-regex target/release/sash-getopt
	(cd $SASH && cargo build --release)
	(cd $CONVERT && cargo build --release)

target/release/sash-regex: sash-rt/csrc/sash-regex.c sash-rt/csrc/regcomp.c sash-rt/csrc/regexec.c sash-rt/csrc/regaux.c
	mkdir -p target/release
	cc -O2 -o target/release/sash-regex sash-rt/csrc/sash-regex.c sash-rt/csrc/regcomp.c sash-rt/csrc/regexec.c sash-rt/csrc/regaux.c -I sash-rt/csrc

target/release/sash-getopt: sash-getopt/sash-getopt.c
	mkdir -p target/release
	cc -O2 -o target/release/sash-getopt sash-getopt/sash-getopt.c

$RT/target/release/libsash_rt.a: $RT/src/lib.rs $CSRC
	(cd $RT && cargo clean -p sash-rt 2>/dev/null; cargo build --release)

test:V: build regex-test sash-grep-test sash-getopt-test status
	(cd $SASH && cargo test --release)
	(cd $CONVERT && cargo test --release)
	(cd sash-expand && cargo test --release)

regex-test:V:
	cc -O0 -o /tmp/sash_test_regex sash-rt/csrc/test_regex.c sash-rt/csrc/regcomp.c sash-rt/csrc/regexec.c sash-rt/csrc/regaux.c -I sash-rt/csrc
	/tmp/sash_test_regex

sash-grep-test:V: build
	PATH=target/release:$$PATH bash contrib/test-regex.sh

sash-getopt-test:V: target/release/sash-getopt
	bash contrib/test-getopt.sh

spec:V: build
	bash spec/run.sh

clean:V:
	(cd $RT && cargo clean)
	(cd $SASH && cargo clean)
	(cd $CONVERT && cargo clean)

fmt:V:
	(cd $SASH && cargo fmt --check)
	(cd $CONVERT && cargo fmt --check)

lint:V:
	(cd $SASH && cargo clippy -- -D warnings)
	(cd $CONVERT && cargo clippy -- -D warnings)

# Coverage. All the fiddly parts (forcing a real instrumented rebuild, the
# canary that proves it, and the SASH_RELEASE_DIR contract with spec/run.sh)
# live in contrib/coverage.sh — the previous recipes set `SASH=`, which
# spec/run.sh does not read, and used `$PWD`, which inside a mkfile is a mk
# variable that expands empty. Both measured the wrong binary silently.
cov:V:
	bash contrib/coverage.sh

cov-branch:V:
	bash contrib/coverage.sh --branch

cov-unit:V:
	bash contrib/coverage.sh --with-unit

cov-rpt:V:
	(cd $SASH && cargo llvm-cov report --release)

# Undo a coverage run. `mk build` alone does NOT: cargo-llvm-cov passes
# -Cinstrument-coverage through a RUSTC_WRAPPER, so cargo's fingerprint never
# sees it and the instrumented artifacts look fresh. Leaving them in place
# means a slower sash that drops default_*.profraw wherever it runs.
cov-reset:V:
	rm -rf target/release
	rm -f default_*.profraw target/*.profraw
	mk build

complexity:V:
	sqlite3 .indexnudge-rust.db "SELECT cyclomatic AS cc, name, file||':'||start_line AS where_ FROM symbols WHERE kind IN ('function','method') AND cyclomatic IS NOT NULL AND file LIKE 'sash/src/%' ORDER BY cyclomatic DESC LIMIT 20;"

status:V:
	bash contrib/status-update.sh

install-hooks:V:
	git config core.hooksPath .githooks
	echo "git hooks installed (.githooks/). To uninstall: git config --unset core.hooksPath"

check:V: lint test complexity
