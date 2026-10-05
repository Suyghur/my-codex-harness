.PHONY: build check test fmt fmt-check

CARGO ?= cargo

build:
	$(CARGO) build --release --locked

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

check: fmt-check
	$(CARGO) clippy --all-targets --locked -- -D warnings
	$(CARGO) test --locked
	$(CARGO) run --locked -- check --scenarios
	git diff --check

test:
	$(CARGO) test --locked
