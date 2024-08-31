.DEFAULT_GOAL := build

fmt:
	cargo fmt
.PHONY:fmt

# `rustup component add clippy`
lint:
	cargo clippy
.PHONY: lint

build: fmt lint
	cargo build
.PHONY: build

release: fmt lint
	cargo build --release
.PHONY: release

run: fmt lint
	# it requires `cargo-watch` via `make deps`
	cargo watch -x 'run' --ignore 'src/tests_integration/*'
.PHONY: run

clean:
	cargo clean
.PHONY: clean

doc:
	cargo rustdoc
.PHONY: doc

test:
	# append `-- --nocapture` to `cargo test` command to show output in console also on success
	ENV=testing RUST_BACKTRACE=full cargo test -- --nocapture --test-threads 1
.PHONY: test

test-coverage:
	# test coverage documentation https://doc.rust-lang.org/rustc/instrument-coverage.html
	# test coverage tutorial https://blog.rng0.io/how-to-do-code-coverage-in-rust
	# you need both 'grcov' and 'llvm-tools-preview' to run tests with coverage
	rm -rf coverage
	mkdir -p coverage/html
	# run test instrumenting for code coverage
	# append `-- --nocapture` to `cargo test` command to show output in console also on success
	ENV=testing RUST_BACKTRACE=full CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE='./coverage/cargo-test-%p-%m.profraw' cargo test -- --nocapture --test-threads 1
	grcov . --binary-path ./target/debug/ -s . -t html --branch \
			--ignore-not-existing \
            --ignore "src/tests_integration/*" \
            --ignore "target/*" \
			--excl-start '^\#\[cfg\(test\)\]' \
            --excl-stop '^}' \
            -o coverage/html
	# if you want, you can emit lcov report or other via -t parameter
	# grcov . --binary-path ./target/debug/deps/ -s . -t lcov --branch --ignore-not-existing --ignore "src/tests/*" -o coverage/tests.lcov
.PHONY: coverage

deps: deps-test
	rustup update
	rustup component add clippy
	rustup component add rustfmt
	cargo update
	cargo install cargo-watch
.PHONY: deps

deps-ci: deps-test
	rustup component add clippy
	rustup component add rustfmt
.PHONY: deps-ci

deps-test:
	# install mozilla/grcov and llvm-tools-preview to show test code coverage
	cargo install grcov
	rustup component add llvm-tools-preview
.PHONE: deps-test
