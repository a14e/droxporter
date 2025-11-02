# Simple Makefile for droxporter

.PHONY: fmt test clippy build run clean check all help

# Format code
fmt:
	cargo fmt

# Run tests
test:
	cargo test

# Run clippy lints
clippy:
	cargo clippy --all-targets --all-features -- -W warnings

# Build project
build:
	cargo build --release

# Run project
run:
	cargo run

# Format, lint and test
check: fmt test clippy

# Clean build artifacts
clean:
	cargo clean

# All checks + build
all: check build

# Help
help:
	@echo "Available commands:"
	@echo "  fmt     - Format code"
	@echo "  test    - Run tests"
	@echo "  clippy  - Run clippy lints"
	@echo "  build   - Build release version"
	@echo "  run     - Run project"
	@echo "  check   - Run fmt, test and clippy"
	@echo "  clean   - Clean build artifacts"
	@echo "  all     - Run all checks and build"
	@echo "  help    - Show this help"