# Automated Release System

This repository has been configured with GitHub Actions workflows for automated release versioning, which can automatically build, package, and publish new versions.

## 🚀 Quick Start

### Initial Setup (One-time)

1. **Configure Crates.io Token**:
   ```bash
   # 1. Visit https://crates.io/me to get API token
   # 2. Add CARGO_REGISTRY_TOKEN secret in GitHub repository settings
   ```

2. **Ensure correct permissions**:
   - In repository Settings > Actions > General
   - Workflow permissions: "Read and write permissions"

### Release New Version

**Just 3 commands!**

```bash
# 1. Update version number (run in Git Bash/WSL)
./scripts/update-version.sh 0.4.2

# 2. Commit and push
git add -A && git commit -m "Release v0.4.2" && git tag v0.4.2 && git push origin master --tags

# 3. Wait for automatic release completion 🎉
```

It's that simple! GitHub Actions will automatically:
- ✅ Build multi-platform binary files
- ✅ Create GitHub Release
- ✅ Generate Changelog
- ✅ Publish to Crates.io
- ✅ Upload compiled files

### Check Release Status

1. **GitHub Actions**: Go to `Actions` page to view build status
2. **GitHub Releases**: Go to `Releases` page to see newly published versions
3. **Crates.io**: Check https://crates.io/crates/rust2go

## Features

- 🚀 **Auto-trigger**: Automatically create releases when version tags are pushed
- 📦 **Multi-platform builds**: Support for Linux, Windows, macOS (x64 and ARM64)
- 📝 **Auto-generated changelog**: Automatically generate update logs based on git commit history
- 🔗 **Publish to crates.io**: Automatically publish all crates in the workspace
- 📎 **Binary releases**: Automatically build and upload compiled binary files

## Release Methods

### Method 1: Using Version Update Script (Recommended)

1. **Run the version update script**:
   ```bash
   # Run in Git Bash or WSL
   ./scripts/update-version.sh 0.4.2
   ```

2. **Check updates**:
   ```bash
   git diff
   ```

3. **Test build**:
   ```bash
   cargo build
   cargo test
   ```

4. **Commit and push**:
   ```bash
   git add -A
   git commit -m "Bump version to 0.4.2"
   git tag v0.4.2
   git push origin master --tags
   ```

### Method 2: Manual Version Update

1. **Manually update version numbers** in each Cargo.toml file
2. **Update dependency versions** ensure internal dependency version numbers are consistent
3. **Run `cargo update`** to update Cargo.lock
4. **Commit and create tag**

### Method 3: Manual Release Trigger

If you want to create a release for an existing commit, you can manually trigger it on the GitHub Actions page:

1. Visit the GitHub repository's Actions page
2. Select the "Release" workflow
3. Click "Run workflow"
4. Enter the version number to release (e.g., v0.4.2)

## Advanced Usage

### Release Preview Version
```bash
./scripts/update-version.sh 0.4.2-beta.1
git add -A && git commit -m "Release v0.4.2-beta.1" && git tag v0.4.2-beta.1 && git push origin master --tags
```

### Test Build Only
```bash
# Run CI tests
git push origin your-branch
```

## Workflow Details

### Trigger Conditions

- **Auto-trigger**: When tags in the format `v*.*.*` are pushed
- **Manual trigger**: Manually run on the GitHub Actions page

### Build Targets

- `x86_64-unknown-linux-gnu` (Linux x64)
- `x86_64-pc-windows-gnu` (Windows x64) 
- `x86_64-apple-darwin` (macOS x64)
- `aarch64-apple-darwin` (macOS ARM64)

### Release Content

Each release will include:

1. **GitHub Release**:
   - Auto-generated changelog
   - Multi-platform binary packages
   - Source code archives

2. **Crates.io Publishing** (stable versions only):
   - All crates in the workspace
   - Automatically published in dependency order

## Prerequisites

### Repository Secrets

For automated publishing to work properly, you need to configure the following secrets in the GitHub repository settings:

1. **CARGO_REGISTRY_TOKEN**: 
   - Create an API token at [crates.io](https://crates.io/me)
   - Add it in GitHub repository Settings > Secrets and variables > Actions

### Permission Settings

Ensure GitHub Actions has sufficient permissions:
- Contents: Write (to create releases)
- Metadata: Read (to read repository information)

## Version Management Recommendations

### Version Number Standards

Follow [Semantic Versioning](https://semver.org/):
- `MAJOR.MINOR.PATCH` (e.g., 1.0.0)
- `MAJOR.MINOR.PATCH-prerelease` (e.g., 1.0.0-beta.1)

### Release Types

- **Stable versions**: `v1.0.0` - Automatically published to crates.io
- **Pre-release versions**: `v1.0.0-beta.1` - Only creates GitHub release, marked as prerelease

### Workspace Version Synchronization

All crates in the workspace should maintain the same version number for:
- Easy management and maintenance
- Ensuring correct dependency relationships
- Simplifying the release process

## Troubleshooting

### Common Issues

1. **Build failures**: Check compilation dependencies for all platforms
2. **Crates.io publishing failures**: Check if CARGO_REGISTRY_TOKEN is set correctly
3. **Permission issues**: Ensure Actions have write permissions
4. **Permission errors**: Check repository's Actions permission settings

### Debugging

- View Actions logs for detailed error information
- Local test builds: `cargo build --release --target <target>`
- Verify version number format and dependency relationships

## Example Workflow

```bash
# 1. Update version
./scripts/update-version.sh 0.5.0

# 2. Check changes
git diff

# 3. Test
cargo test

# 4. Commit and release
git add -A
git commit -m "Release version 0.5.0"
git tag v0.5.0
git push origin master --tags

# 5. Wait for GitHub Actions to complete build and release
```

After the release is complete, you can see the new release on the GitHub releases page, and the updated crates will be available on crates.io. 