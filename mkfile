RT=sash-rt
SASH=sash
CONVERT=sash-convert
CSRC=`{ls sash-rt/csrc/*.c sash-rt/csrc/*.S sash-rt/csrc/*.h}

build:V: $RT/target/release/libsash_rt.a
	(cd $SASH && cargo build --release)
	(cd $CONVERT && cargo build --release)

$RT/target/release/libsash_rt.a: $RT/src/lib.rs $CSRC
	(cd $RT && cargo clean -p sash-rt 2>/dev/null; cargo build --release)

test:V: build
	(cd $SASH && cargo test --release)

spec:V: build
	bash spec/run.sh

clean:V:
	(cd $RT && cargo clean)
	(cd $SASH && cargo clean)
	(cd $CONVERT && cargo clean)
