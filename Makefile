.PHONY: check build release release-fast clean test clippy fmt fmt-check help

# Default target
.DEFAULT_GOAL := help

# Help target
help:
	@echo "Available targets:"
	@echo "  check      - Check code without building (cargo check)"
	@echo "  build      - Build debug binary (cargo build)"
	@echo "  release    - Build full-LTO release binary (cargo build --release)"
	@echo "  release-fast - Build faster release-like binary (cargo build --profile release-fast)"
	@echo "  test       - Run tests (cargo test)"
	@echo "  clean      - Remove build artifacts (cargo clean)"
	@echo "  clippy     - Run clippy linter (cargo clippy)"
	@echo "  fmt        - Format code (cargo fmt)"
	@echo "  fmt-check  - Check code formatting (cargo fmt -- --check)"

# Check code
check:
	cargo check

# Build debug
build:
	cargo build

# Build release
release:
	cargo build --release

# Build fast release-like profile
release-fast:
	cargo build --profile release-fast

# Run tests
test:
	cargo test

# Clean build artifacts
clean:
	cargo clean

# Run clippy
clippy:
	cargo clippy -- -D warnings

# Format code
fmt:
	cargo fmt

# Check formatting
fmt-check:
	cargo fmt -- --check