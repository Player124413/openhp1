#!/usr/bin/env bash
set -euo pipefail

# OpenHP1 Android Build Script
# Builds shared native libraries for ARM64, ARMv7, and x86_64, then packages APK.

readonly TARGET_ARM64="aarch64-linux-android"
readonly TARGET_ARMV7="armv7-linux-androideabi"
readonly TARGET_X86_64="x86_64-linux-android"

readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
readonly ANDROID_DIR="$REPO_ROOT/android"
readonly JNI_LIBS_DIR="$ANDROID_DIR/app/src/main/jniLibs"

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

info() {
    printf '==> %s\n' "$*"
}

# Check for NDK
if [[ -z "${ANDROID_NDK_HOME:-}" && -z "${NDK_HOME:-}" ]]; then
    if [[ -d "$HOME/Android/Sdk/ndk" ]]; then
        latest_ndk="$(ls -1d "$HOME/Android/Sdk/ndk/"* 2>/dev/null | sort -V | tail -n 1 || true)"
        if [[ -n "$latest_ndk" ]]; then
            export ANDROID_NDK_HOME="$latest_ndk"
        fi
    fi
fi

readonly NDK_DIR="${ANDROID_NDK_HOME:-${NDK_HOME:-}}"
if [[ -z "$NDK_DIR" || ! -d "$NDK_DIR" ]]; then
    echo "warning: ANDROID_NDK_HOME is not set or not a directory. Continuing if cargo-ndk is installed..."
fi

build_mode="release"
cargo_flags=("--release")
if [[ "${1:-}" == "--debug" ]]; then
    build_mode="debug"
    cargo_flags=()
fi

mkdir -p "$JNI_LIBS_DIR/arm64-v8a"
mkdir -p "$JNI_LIBS_DIR/armeabi-v7a"
mkdir -p "$JNI_LIBS_DIR/x86_64"

info "Building OpenHP1 for Android ($build_mode)..."

build_target() {
    local target="$1"
    local abi="$2"

    info "Building for $abi ($target)..."

    if command -v cargo-ndk >/dev/null 2>&1; then
        cargo ndk --target "$target" --platform 24 build -p openhp1-game "${cargo_flags[@]}"
    else
        cargo build --target "$target" -p openhp1-game "${cargo_flags[@]}"
    fi

    local src_lib="$REPO_ROOT/target/$target/$build_mode/libopenhp1_game.so"
    if [[ -f "$src_lib" ]]; then
        cp -v "$src_lib" "$JNI_LIBS_DIR/$abi/libopenhp1_game.so"
    else
        echo "warning: Could not find $src_lib"
    fi
}

build_target "$TARGET_ARM64" "arm64-v8a"

if [[ "${BUILD_ALL_ABIS:-0}" == "1" ]]; then
    build_target "$TARGET_ARMV7" "armeabi-v7a"
    build_target "$TARGET_X86_64" "x86_64"
fi

info "Native libraries successfully placed in $JNI_LIBS_DIR"

if [[ -f "$ANDROID_DIR/gradlew" ]]; then
    info "Assembling Android APK..."
    (
        cd "$ANDROID_DIR"
        chmod +x gradlew
        if [[ "$build_mode" == "release" ]]; then
            ./gradlew assembleRelease
        else
            ./gradlew assembleDebug
        fi
    )
    info "APK build complete!"
else
    info "To assemble APK, run Gradle in $ANDROID_DIR or open project in Android Studio."
fi
