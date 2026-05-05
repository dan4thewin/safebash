RT=sash-rt
SASH=sash
CONVERT=sash-convert

build:V:
	(cd $RT && cargo build --release)
	(cd $SASH && cargo build --release)
	(cd $CONVERT && cargo build --release)

test:V: build
	(cd $SASH && cargo test --release)

spec:V: build
	bash spec/run.sh

clean:V:
	(cd $RT && cargo clean)
	(cd $SASH && cargo clean)
	(cd $CONVERT && cargo clean)
