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

cov-clean:V:
	(cd $SASH && cargo llvm-cov clean --workspace) >/dev/null 2>&1

cov-unit:V:
	(cd $SASH && cargo llvm-cov --no-report --release --lib \
		--ignore-run-fail) 2>&1 | tail -1

cov-spec:V:
	(cd $SASH && cargo llvm-cov run --release --bin sash ) >/dev/null 2>&1
	LLVM_PROFILE_FILE="$PWD/target/llvm-cov-target/spec-%p-%m.profraw" \
		SASH="$PWD/target/llvm-cov-target/release/sash" \
		bash spec/run.sh >/dev/null 2>&1

cov-rpt:V:
	(cd $SASH && cargo llvm-cov report --release)

cov-full:V:
	mk cov-clean; mk cov-unit cov-spec; mk cov-rpt

cov-spec-only:V:
	mk cov-clean; mk cov-spec; mk cov-rpt

complexity:V:
	sqlite3 .indexnudge-rust.db "SELECT cyclomatic AS cc, name, file||':'||start_line AS where_ FROM symbols WHERE kind IN ('function','method') AND cyclomatic IS NOT NULL AND file LIKE 'sash/src/%' ORDER BY cyclomatic DESC LIMIT 20;"

status:V:
	bash contrib/status-update.sh

install-hooks:V:
	git config core.hooksPath .githooks
	echo "git hooks installed (.githooks/). To uninstall: git config --unset core.hooksPath"

check:V: lint test complexity
