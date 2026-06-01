RT=sash-rt
SASH=sash
CONVERT=sash-convert
CSRC=`ls sash-rt/csrc/*.c sash-rt/csrc/*.S sash-rt/csrc/*.h | tr '\n' ' '`

build:V: $RT/target/release/libsash_rt.a
	(cd $SASH && cargo build --release)
	(cd $CONVERT && cargo build --release)

$RT/target/release/libsash_rt.a: $RT/src/lib.rs $CSRC
	(cd $RT && cargo clean -p sash-rt 2>/dev/null; cargo build --release)

test:V: build regex-test sash-grep-test status
	(cd $SASH && cargo test --release)
	(cd $CONVERT && cargo test --release)
	(cd sash-expand && cargo test --release)

regex-test:V:
	cc -O0 -o /tmp/sash_test_regex sash-rt/csrc/test_regex.c sash-rt/csrc/regcomp.c sash-rt/csrc/regexec.c sash-rt/csrc/regaux.c -I sash-rt/csrc
	/tmp/sash_test_regex

sash-grep-test:V: build
	PATH=target/release:$$PATH bash contrib/test-regex.sh

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

coverage:V: build
	(cd $SASH && cargo llvm-cov clean --workspace) >/dev/null 2>&1
	(cd $SASH && cargo llvm-cov --no-report --release \
		--test e2e_tests --test oracle_tests --test taint_spec_tests \
		--test janet_tests --test codegen_tests --test brush_compat_tests \
		--ignore-run-fail) 2>&1 | tail -1
	LLVM_PROFILE_FILE="`pwd`/target/llvm-cov-target/spec-%p-%m.profraw" \
		PATH="`pwd`/target/llvm-cov-target/release:$$PATH" \
		bash spec/run.sh >/dev/null 2>&1
	(cd $SASH && cargo llvm-cov report --release)

complexity:V:
	sqlite3 .indexnudge-rust.db "SELECT cyclomatic AS cc, name, file||':'||start_line AS where_ FROM symbols WHERE kind IN ('function','method') AND cyclomatic IS NOT NULL AND file LIKE 'sash/src/%' ORDER BY cyclomatic DESC LIMIT 20;"

status:V:
	bash contrib/status-update.sh

install-hooks:V:
	git config core.hooksPath .githooks
	echo "git hooks installed (.githooks/). To uninstall: git config --unset core.hooksPath"

check:V: lint test complexity
