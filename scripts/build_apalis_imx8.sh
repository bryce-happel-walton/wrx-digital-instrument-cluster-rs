#!/bin/bash
set -e

cd "$(dirname "${BASH_SOURCE[0]}")/.."

LATEST_SDK=$(ls -1 /opt/wrx-cluster-rs | sort -V | tail -n1)
SDK_SETUP="/opt/wrx-cluster-rs/${LATEST_SDK}/environment-setup-armv8a-tdx-linux"
TARGET="aarch64-unknown-linux-gnu"
BASE_FEATURES="apalis_imx8,wayland"

# Usage: ./build_apalis_imx8.sh [profile] [extra,features]
#   profile        - debug (default), release, or any custom profile name
#   extra,features - comma-separated cargo features to append to the baseline
PROFILE="${1:-debug}"
EXTRA_FEATURES="${2:-}"

if [[ -n "$EXTRA_FEATURES" ]]; then
    FEATURES="${BASE_FEATURES},${EXTRA_FEATURES}"
else
    FEATURES="$BASE_FEATURES"
fi

if [[ ! -f "$SDK_SETUP" ]]; then
    printf "\tERROR: Yocto SDK environment setup script not found at:\n"
    printf "\t\t$SDK_SETUP\n"
    printf "\tYou must build and install the SDK before running this script.\n"
    printf "\tTo build the SDK, run on your Yocto build machine:\n"
    printf "\t\t$ bitbake image-hr-testing-rs -c populate_sdk\n"
    printf "\tThen install the SDK as instructed by Yocto.\n"
    exit 1
fi

printf "[build 1/2] Sourcing Yocto SDK...\n"
printf "\tUsing SDK: $SDK_SETUP\n"

HOST_TRIPLE="$(rustc -vV | awk '/^host:/ {print $2}')"
HOST_TRIPLE_UNDERSCORE="${HOST_TRIPLE//-/_}"
HOST_CC_DETECTED="$(command -v cc || command -v gcc)"
HOST_CXX_DETECTED="$(command -v c++ || command -v g++)"
HOST_AR_DETECTED="$(command -v ar)"

source "$SDK_SETUP"

export "HOST_CC=$HOST_CC_DETECTED"
export "HOST_CXX=$HOST_CXX_DETECTED"
export "CC_${HOST_TRIPLE_UNDERSCORE}=$HOST_CC_DETECTED"
export "CXX_${HOST_TRIPLE_UNDERSCORE}=$HOST_CXX_DETECTED"
export "AR_${HOST_TRIPLE_UNDERSCORE}=$HOST_AR_DETECTED"
export "CFLAGS_${HOST_TRIPLE_UNDERSCORE}="
export "CXXFLAGS_${HOST_TRIPLE_UNDERSCORE}="

if ! rustup target list --installed | grep -q "^$TARGET$"; then
    printf "[build 1b/2] Installing Rust target '$TARGET'...\n"
    rustup target add "$TARGET"
fi

printf "[build 2/2] Building Rust project with profile '$PROFILE' for $TARGET...\n"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-tdx-linux-gcc
export RUSTFLAGS="-C link-arg=--sysroot=$SDKTARGETSYSROOT"
BUILD_EXTRA_FLAGS="--no-default-features --features $FEATURES"

if [[ "$PROFILE" == "debug" ]]; then
    cargo build --target $TARGET $BUILD_EXTRA_FLAGS
    PROFILE_DIR="debug"
elif [[ "$PROFILE" == "release" ]]; then
    cargo build --target $TARGET --release $BUILD_EXTRA_FLAGS
    PROFILE_DIR="release"
else
    cargo build --target $TARGET --profile $PROFILE $BUILD_EXTRA_FLAGS
    PROFILE_DIR="$PROFILE"
fi
