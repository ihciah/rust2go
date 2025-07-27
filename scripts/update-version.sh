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

# Define packages to update
declare -A PACKAGES=(
    ["rust2go-common"]=""
    ["rust2go-convert"]=""
    ["rust2go-macro"]=""
    ["rust2go-mem-ffi"]=""
    ["mem-ring"]=""
    ["rust2go-cli"]=""
    ["rust2go"]=""
)

# Define package dependencies
declare -A DEPENDENCIES=(
    ["rust2go"]="rust2go-macro rust2go-convert rust2go-cli"
    ["rust2go-cli"]=""
    ["rust2go-macro"]=""
    ["rust2go-convert"]=""
    ["rust2go-mem-ffi"]=""
    ["mem-ring"]=""
    ["rust2go-common"]=""
)

echo "Updating version numbers..."

# Update each package's version
for package in "${!PACKAGES[@]}"; do
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

for package in "${!DEPENDENCIES[@]}"; do
    toml_file="${package}/Cargo.toml"
    if [ -f "$toml_file" ]; then
        deps="${DEPENDENCIES[$package]}"
        for dep in $deps; do
            echo "Updating dependency $dep in $package"
            # Update path dependency versions
            # Use simpler approach with variables to improve readability
            pattern_basic="^$dep = { version = \"[^\"]*\", path = \"\\.\\.\\/$dep\" }"
            replacement_basic="$dep = { version = \"$NEW_VERSION\", path = \"\\.\\.\\/$dep\" }"
            sed -i.bak "s/$pattern_basic/$replacement_basic/" "$toml_file"
            
            pattern_optional="^$dep = { version = \"[^\"]*\", path = \"\\.\\.\\/$dep\", optional = true }"
            replacement_optional="$dep = { version = \"$NEW_VERSION\", path = \"\\.\\.\\/$dep\", optional = true }"
            sed -i.bak "s/$pattern_optional/$replacement_optional/" "$toml_file"
            
            rm "$toml_file.bak"
        done
    fi
done

# Update Cargo.lock
echo "Updating Cargo.lock..."
cargo update

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