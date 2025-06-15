#!/bin/bash
# Escape on failure
set -e
OPTIONS=cb
LONG_OPTIONS=cuda,both
BUILD_FEATURES=None
CUDA_OUTPUT="target/cuda/"
CUDA_BINARY="{ path = \"$PWD/target/cuda/release/ribble\", main = true }"
CPU_OUTPUT="target/cpu/"
CPU_BINARY="{ path = \"$PWD/target/cpu/release/ribble\", main = true }"
CPU_DEPENDS="libstdc++6\", \"libssl3\", \"libgcc1\", \"libc6"
CUDA_DEPENDS="libstdc++6\", \"libssl3\", \"libgcc1\", \"libc6\", \"cuda-toolkit-12-4"
CPU_LIBS="libstdc++.so.6\", \"libssl.so.3\", \"libcrypto.so.3\", \"libgcc_s.so.1\", \"libm.so.6\", \"libc.so.6"
CUDA_LIBS="libstdc++.so.6\", \"libssl.so.3\", \"libcrypto.so.3\", \"libgcc_s.so.1\", \"libm.so.6\", \"libc.so.6\",  \"libcudart.so.12\", \"libcublas.so.12\", \"libcublasLt.so.12"

function cleanup(){
  if [[ -f "Packager.toml.bak" ]]; then
    mv Packager.toml.bak Packager.toml
  fi
}

trap cleanup EXIT

build_cuda() {
    if [ ! -d "$CUDA_OUTPUT" ]; then
      mkdir -p "$CUDA_OUTPUT"
    fi
    cargo build --release --target-dir "$CUDA_OUTPUT" --features cuda
    cp Packager.toml Packager.toml.bak
    sed "s|\"DEPENDS\"|\"$CUDA_DEPENDS\"|g; s|\"LIBS\"|\"$CUDA_LIBS\"|g; ;s|\"BINARY\"|$CUDA_BINARY|g" Packager.toml > m_Packager.toml
    mv m_Packager.toml Packager.toml
    # TODO: this should only package debian; appimage is too large
    cargo packager

    mv Packager.toml.bak Packager.toml

    if [ ! -d "$PWD/dist/cuda" ]; then
      mkdir -p "$PWD/dist/cuda"
    fi

    while read -r file; do
      ext="${file##*.}";
      filename="${file%.*}";
      mv "$PWD/dist/$file" "$PWD/dist/cuda/${filename}_cuda.${ext}";
    done < <(ls "$PWD/dist/" | grep "[Rr]ibble")
}

build_cpu(){
    if [ ! -d "$CPU_OUTPUT" ]; then
      mkdir -p "$CPU_OUTPUT"
    fi
    cargo build --release --target-dir "$CPU_OUTPUT"
    cp Packager.toml Packager.toml.bak
    sed "s|\"DEPENDS\"|\"$CPU_DEPENDS\"|g; s|\"LIBS\"|\"$CPU_LIBS\"|g; ;s|\"BINARY\"|$CPU_BINARY|g" Packager.toml > m_Packager.toml
    mv m_Packager.toml Packager.toml
    cargo packager
    if [ ! -d "$PWD/dist/cpu" ]; then
      mkdir -p "$PWD/dist/cpu"
    fi

    while read -r file; do
      mv "$PWD/dist/$file" "$PWD/dist/cpu/$file";
    done < <(ls "$PWD/dist/" | grep "[Rr]ibble")

    mv Packager.toml.bak Packager.toml
}

if PARSED=$(getopt --options "$OPTIONS" --longoptions "$LONG_OPTIONS" --name "$0" -- "$@"); then
  eval set -- "$PARSED"
  else
    echo "Failed to parse options" >&2
    exit 1
fi

while [ "$1" ]; do
  case "$1" in
    -c|--cuda)
    echo "Building CUDA"
    BUILD_FEATURES=cuda
    ;;
    -b|--both)
    echo "Building both"
    BUILD_FEATURES=both
    ;;
    --)
      break
    ;;
    *)
      echo "Invalid option: $1"
      exit 1
      ;;
  esac
  shift
done

rustup target add x86_64-unknown-linux-gnu

case $BUILD_FEATURES in
  cuda)
    build_cuda
    ;;
  both)
    if [ ! -d "$CUDA_OUTPUT" ]; then
      mkdir -p "$CUDA_OUTPUT"
    fi

    cargo build --release
    build_cuda
    build_cpu
    ;;
  *)
    build_cpu
    ;;
esac
