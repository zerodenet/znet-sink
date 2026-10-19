#!/usr/bin/env bash
# Source before Cargo/Tauri so bindgen reads the real NDK headers.
: "${ANDROID_NDK_HOME:?Android NDK required}"
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) znet_ndk_host=linux-x86_64 ;;
  Darwin-*) znet_ndk_host=darwin-x86_64 ;;
  *) echo 'Unsupported NDK build host' >&2; return 1 ;;
esac
znet_ndk_bin="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$znet_ndk_host/bin"
test -x "$znet_ndk_bin/aarch64-linux-android24-clang" || return 1
export PATH="$znet_ndk_bin:$PATH"
export CC_aarch64_linux_android="$znet_ndk_bin/aarch64-linux-android24-clang"
export CXX_aarch64_linux_android="$znet_ndk_bin/aarch64-linux-android24-clang++"
export AR_aarch64_linux_android="$znet_ndk_bin/llvm-ar"
unset znet_ndk_host znet_ndk_bin
