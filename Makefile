# Makefile for sortify-rs (Standalone Rust CLI)

# Variables
CARGO = cargo
INSTALL_DIR = $(HOME)/.local/bin
BINARY_NAME = sortify-rs
TARGET_DIR = target
RELEASE_BINARY = $(TARGET_DIR)/release/$(BINARY_NAME)

# Default target
.PHONY: all
all: build

# Build the release binary
.PHONY: build
build:
	@echo "Building $(BINARY_NAME) in release mode..."
	$(CARGO) build --release
	@echo "Build complete: $(RELEASE_BINARY)"

# Install the binary to ~/.local/bin/
.PHONY: install
install: build
	@echo "Installing $(BINARY_NAME) to $(INSTALL_DIR)/"
	@mkdir -p $(INSTALL_DIR)
	@cp $(RELEASE_BINARY) $(INSTALL_DIR)/
	@chmod +x $(INSTALL_DIR)/$(BINARY_NAME)
	@echo "Installation complete!"
	@echo "Binary installed to: $(INSTALL_DIR)/$(BINARY_NAME)"
	@echo ""
	@echo "Make sure $(INSTALL_DIR) is in your PATH:"
	@echo "  export PATH=\"\$$HOME/.local/bin:\$$PATH\""

# Uninstall the binary
.PHONY: uninstall
uninstall:
	@echo "Uninstalling $(BINARY_NAME) from $(INSTALL_DIR)/"
	@rm -f $(INSTALL_DIR)/$(BINARY_NAME)
	@echo "Uninstallation complete!"

# Clean build artifacts
.PHONY: clean
clean:
	@echo "Cleaning build artifacts..."
	$(CARGO) clean
	@echo "Clean complete!"

# Run tests
.PHONY: test
test:
	@echo "Running tests..."
	$(CARGO) test
	@echo "Tests complete!"

# Check code without building
.PHONY: check
check:
	@echo "Checking code..."
	$(CARGO) check
	@echo "Check complete!"

# Run clippy lints
.PHONY: clippy
clippy:
	@echo "Running clippy..."
	$(CARGO) clippy -- -D warnings
	@echo "Clippy complete!"

# Format code
.PHONY: fmt
fmt:
	@echo "Formatting code..."
	$(CARGO) fmt
	@echo "Format complete!"

# Show help
.PHONY: help
help:
	@echo "Available targets:"
	@echo "  build     - Build the release binary"
	@echo "  install   - Build and install to ~/.local/bin/"
	@echo "  uninstall - Remove binary from ~/.local/bin/"
	@echo "  clean     - Remove build artifacts"
	@echo "  test      - Run tests"
	@echo "  check     - Check code without building"
	@echo "  clippy    - Run clippy lints"
	@echo "  fmt       - Format code"
	@echo "  help      - Show this help"
	@echo ""
	@echo "Usage examples:"
	@echo "  make build     # Build the binary"
	@echo "  make install   # Build and install"
	@echo "  make uninstall # Remove from system"
