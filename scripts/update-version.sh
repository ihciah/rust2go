#!/bin/bash

# Version update script for Rust2Go
# Usage: ./scripts/update-version.sh <new_version>
# Examples: 
#   ./scripts/update-version.sh v0.4.2    (Go format - recommended)
#   ./scripts/update-version.sh 0.4.2     (Rust format)

set -e

INPUT_VERSION="$1"

if [ -z "$INPUT_VERSION" ]; then
    echo "Usage: $0 <new_version>"
    echo "Examples:"
    echo "  $0 v0.4.2    (Go format - recommended)"
    echo "  $0 0.4.2     (Rust format)"
    exit 1
fi

# Handle Go format (v-prefixed) or Rust format
if [[ "$INPUT_VERSION" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.-]+)?$ ]]; then
    GIT_VERSION="$INPUT_VERSION"
    RUST_VERSION="${INPUT_VERSION#v}"  # Remove 'v' prefix
    echo "🎯 Go format detected: $GIT_VERSION"
elif [[ "$INPUT_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.-]+)?$ ]]; then
    RUST_VERSION="$INPUT_VERSION"
    GIT_VERSION="v$INPUT_VERSION"  # Add 'v' prefix
    echo "🦀 Rust format detected: $RUST_VERSION, will use Git tag: $GIT_VERSION"
else
    echo "Error: Invalid version format. Should be:"
    echo "  v1.2.3 or v1.2.3-suffix (Go format)"
    echo "  1.2.3 or 1.2.3-suffix (Rust format)"
    exit 1
fi

echo "Preparing to update Rust crates to version $RUST_VERSION..."
echo "Git tag will be: $GIT_VERSION"

# Use RUST_VERSION for updating Cargo.toml files
NEW_VERSION="$RUST_VERSION"

# Get current version
CURRENT_VERSION=$(grep '^version = ' rust2go/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
echo "Current version: $CURRENT_VERSION"

# Packages to update (ordered by dependency: leaves first)
PACKAGES="rust2go-common rust2go-convert rust2go-macro mem-ring rust2go-mem-ffi rust2go-cli rust2go"

# Intra-workspace path dependencies of each package
# (bash 3.2 compatible: no associative arrays, works on macOS default bash)
deps_of() {
    case "$1" in
        rust2go) echo "rust2go-macro rust2go-convert rust2go-cli" ;;
        rust2go-cli) echo "rust2go-common" ;;
        rust2go-macro) echo "rust2go-common" ;;
        rust2go-mem-ffi) echo "mem-ring rust2go-convert" ;;
        *) echo "" ;;
    esac
}

echo "Updating version numbers..."

# Update each package's version
for package in $PACKAGES; do
    toml_file="${package}/Cargo.toml"
    if [ -f "$toml_file" ]; then
        echo "Updating $toml_file"
        # Update package's own version
        sed -i.bak "s/^version = \".*\"/version = \"$NEW_VERSION\"/" "$toml_file"
        rm "$toml_file.bak"
    fi
done

# Update dependency versions
echo "Updating dependency versions..."

for package in $PACKAGES; do
    toml_file="${package}/Cargo.toml"
    if [ -f "$toml_file" ]; then
        deps="$(deps_of "$package")"
        for dep in $deps; do
            echo "Updating dependency $dep in $package"
            # Replace only the version string of the path dependency, keeping
            # any trailing keys (optional, default-features, features, ...).
            pattern="^$dep = { version = \"[^\"]*\""
            replacement="$dep = { version = \"$NEW_VERSION\""
            sed -i.bak "s/$pattern/$replacement/" "$toml_file"
            rm "$toml_file.bak"
        done
    fi
done

# Sync Cargo.lock workspace member versions (without touching external deps
# and without requiring a full toolchain / Go for build scripts)
echo "Updating Cargo.lock..."
cargo metadata --format-version 1 --quiet > /dev/null

echo "Version update completed!"
echo ""
echo "📋 Next steps:"
echo "1. Check the updates: git diff"
echo "2. Build and test: cargo build && cargo test"
echo "3. Commit changes: git add -A && git commit -m \"Bump version to $RUST_VERSION\""
echo "4. Create Git tag: git tag $GIT_VERSION"
echo "5. Push changes: git push origin master --tags"
echo ""
echo "🚀 The release workflow will trigger automatically after pushing the tag!"
echo ""
echo "📦 After release, users can install with:"
echo "   Go:   go get github.com/ihciah/rust2go@$GIT_VERSION"
echo "   Rust: cargo add rust2go@$RUST_VERSION" 