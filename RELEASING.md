# Releasing TUI-DB

This document describes how to create a new release of TUI-DB with pre-built binaries for multiple platforms.

## Automated Release Process

The project uses GitHub Actions to automatically build and release binaries for multiple platforms when you push a version tag.

### Supported Platforms

The release workflow builds binaries for:

- **Linux x86_64** (`tui-db-linux-x86_64`)
- **Linux ARM64** (`tui-db-linux-aarch64`)
- **macOS x86_64** (Intel) (`tui-db-macos-x86_64`)
- **macOS ARM64** (Apple Silicon) (`tui-db-macos-aarch64`)
- **Windows x86_64** (`tui-db-windows-x86_64.exe`)

## Creating a Release

### 1. Update Version Number

First, update the version in `Cargo.toml`:

```toml
[package]
name = "tui-db"
version = "0.2.0"  # Update this
edition = "2021"
```

### 2. Commit Changes

```bash
git add Cargo.toml
git commit -m "Bump version to 0.2.0"
git push origin main
```

### 3. Create and Push Tag

Create a git tag with the version number (must start with `v`):

```bash
# Create tag
git tag v0.2.0

# Push tag to GitHub
git push origin v0.2.0
```

### 4. Wait for Build

The GitHub Actions workflow will:
1. Automatically detect the new tag
2. Build binaries for all supported platforms
3. Create a GitHub release with the tag
4. Upload all binaries as release assets
5. Generate release notes from recent commits

You can monitor the progress at: `https://github.com/YOUR_USERNAME/tui-db/actions`

### 5. Edit Release Notes (Optional)

After the release is created, you can:
1. Go to the Releases page: `https://github.com/YOUR_USERNAME/tui-db/releases`
2. Click "Edit" on the new release
3. Add custom release notes, installation instructions, or changelog
4. Add highlights of new features or bug fixes

## Manual Release (Alternative)

If you need to create a release manually:

### Build for Current Platform

```bash
cargo build --release
```

The binary will be at `target/release/tui-db` (or `tui-db.exe` on Windows).

### Cross-Compile for Other Platforms

Install cross-compilation tools:

```bash
# Install cross (supports multiple targets)
cargo install cross

# Build for Linux from any platform
cross build --release --target x86_64-unknown-linux-gnu

# Build for Windows from any platform
cross build --release --target x86_64-pc-windows-gnu

# Build for macOS (requires macOS host or cross-compilation setup)
cargo build --release --target x86_64-apple-darwin
```

## Version Numbering

Follow [Semantic Versioning](https://semver.org/):

- **MAJOR** version (1.0.0): Incompatible API changes
- **MINOR** version (0.2.0): New functionality (backwards-compatible)
- **PATCH** version (0.1.1): Bug fixes (backwards-compatible)

## Pre-releases

For beta or release candidate versions:

```bash
git tag v0.2.0-beta.1
git push origin v0.2.0-beta.1
```

The release will be automatically marked as a pre-release.

## Troubleshooting

### Build Fails for Specific Platform

- Check the Actions logs for the specific platform
- Common issues:
  - Missing system dependencies
  - Cross-compilation toolchain issues
  - Test failures on specific platforms

### Tag Already Exists

If you need to move a tag:

```bash
# Delete local tag
git tag -d v0.2.0

# Delete remote tag
git push origin :refs/tags/v0.2.0

# Create new tag
git tag v0.2.0
git push origin v0.2.0
```

### Release Not Created

Ensure:
1. Tag name starts with `v` (e.g., `v0.2.0`, not `0.2.0`)
2. Repository has Actions enabled
3. GitHub token has appropriate permissions
