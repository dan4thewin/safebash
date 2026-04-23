RT=sash-rt
SASH=sash

build:V:
	cd $RT && cargo build --release
	cd $SASH && cargo build --release

test:V: build
	cd $SASH && cargo test --release

clean:V:
	cd $RT && cargo clean
	cd $SASH && cargo clean
