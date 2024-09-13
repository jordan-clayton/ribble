#! /bin/zsh
# Escape on failure
set -e

function cleanup() {
	if [[ -f "Packager.toml.bak" ]]; then
		mv "Packager.toml.bak" "Packager.toml"
	fi
}

trap cleanup EXIT

SKIP_BUILD=$1

UNIVERSAL_DIR="target/universal/release/"
UNIVERSAL_BIN="{ path = \"$PWD/target/universal/release/ribble\", main = true }"

if [ -z "$SIGNING_IDENTITY" ]; then
	echo "SIGNING_IDENTITY env variable is not set."
	exit 1
fi

if [[ -z "$SKIP_BUILD" || ! "$SKIP_BUILD" == "true" ]]; then
	rustup target add x86_64-apple-darwin
	rustup target add aarch64-apple-darwin
	cargo build --release --target x86_64-apple-darwin --features metal
	cargo build --release --target aarch64-apple-darwin --features metal
fi

if [ ! -d "$UNIVERSAL_DIR" ]; then
	mkdir -p "$UNIVERSAL_DIR"
fi

mkdir -p target/universal/release/

lipo -create \
  target/x86_64-apple-darwin/release/ribble \
  target/aarch64-apple-darwin/release/ribble \
  -output target/universal/release/ribble


codesign --sign "$SIGNING_IDENTITY" target/universal/release/ribble
codesign --verify --deep --strict target/universal/release/ribble

cp Packager.toml Packager.toml.bak

sed "s|\"SIGNING_IDENTITY\"|\"$SIGNING_IDENTITY\"|g; s|\"BINARY\"|$UNIVERSAL_BIN|g" Packager.toml > m_Packager.toml
mv m_Packager.toml Packager.toml

cargo packager

mv Packager.toml.bak Packager.toml