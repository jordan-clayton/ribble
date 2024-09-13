#! /bin/zsh
# Escape on failure
set -e

DMG_PATH="dist/Ribble.dmg"
MOUNT_POINT="/Volumes/Ribble"
UNIVERSAL_DIR="target/universal/release/"

cleanup() {
	if diskutil info "$MOUNT_POINT" | grep "Mounted: Yes" > /dev/null; then
		hdiutil detach "$MOUNT_POINT"
	fi

}

trap cleanup EXIT

if [ -z "$SIGNING_IDENTITY" ]; then
	echo "SIGNING_IDENTITY env variable is not set."
	exit 1
fi


rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin --features metal
cargo build --release --target aarch64-apple-darwin --features metal

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

cargo packager --target universal/release/ribble

hdiutil attach "$DMG_PATH" -mountpoint "$MOUNT_POINT"

codesign --sign "$SIGNING_IDENTITY" --deep --force "$MOUNT_POINT/Ribble.app"
hdiutil detach "$MOUNT_POINT"
