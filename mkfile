RT=sash-rt
SASH=sash
CONVERT=sash-convert
CSRC=`ls sash-rt/csrc/*.c sash-rt/csrc/*.S sash-rt/csrc/*.h | tr '\n' ' '`

build:V: $RT/target/release/libsash_rt.a
	(cd $SASH && cargo build --release)
	(cd $CONVERT && cargo build --release)

$RT/target/release/libsash_rt.a: $RT/src/lib.rs $CSRC
	(cd $RT && cargo clean -p sash-rt 2>/dev/null; cargo build --release)

test:V: build regex-test
	(cd $SASH && cargo test --release)

regex-test:V:
	cc -O0 -o /tmp/sash_test_regex sash-rt/csrc/test_regex.c sash-rt/csrc/regcomp.c sash-rt/csrc/regexec.c sash-rt/csrc/regaux.c -I sash-rt/csrc
	/tmp/sash_test_regex

spec:V: build
	bash spec/run.sh

clean:V:
	(cd $RT && cargo clean)
	(cd $SASH && cargo clean)
	(cd $CONVERT && cargo clean)
