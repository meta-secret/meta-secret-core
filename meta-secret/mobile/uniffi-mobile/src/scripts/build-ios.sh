# Build for iOS device and simulator
build:
	cargo build --target aarch64-apple-ios
	cargo build --target x86_64-apple-ios
	cargo build --target aarch64-apple-ios-sim

generate:
	cargo run --bin uniffi-bindgen generate \
		--library target/aarch64-apple-ios/debug/libswiftandaluh.dylib \
		--language swift \
		--out-dir include
	mv include/swiftandaluhFFI.modulemap include/module.modulemap

all: build generate