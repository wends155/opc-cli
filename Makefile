.PHONY: all debug release build test package clean

all: debug

debug:
	cargo build

release:
	cargo build --release

build: release

test:
	cargo test

# Creates a deployment zip with EXE and debug symbols
package: release
	mkdir -p dist
	cp target/release/opc-cli.exe dist/
	cp target/release/opc-cli.pdb dist/ || true
	cp README.md dist/
	tar -a -c -f opc-cli-dist.zip dist/*

clean:
	cargo clean
	rm -rf dist
