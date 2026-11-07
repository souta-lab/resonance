.PHONY: all build web test check run bench install clean

all: build web

build:
	cargo build --release

web:
	cd web && npm install --no-audit --no-fund --legacy-peer-deps && npm run build

test:
	cargo test --workspace

check:
	cargo check --workspace
	cd web && npx svelte-check --tsconfig ./tsconfig.json

run: build web
	./target/release/resonance daemon

bench: build
	./target/release/resonance bench --seconds 10

install: build web
	./packaging/install.sh

clean:
	cargo clean
	rm -rf web/dist web/node_modules
