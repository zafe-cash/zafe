#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
export ANDROID_NDK_HOME=${ANDROID_NDK_HOME:-$HOME/android/android-ndk-r29}
cargo ndk -t arm64-v8a build --release -p zafe-core --example mobile_bench
BIN=target/aarch64-linux-android/release/examples/mobile_bench
adb push "$BIN" /data/local/tmp/mobile_bench >/dev/null
adb shell chmod +x /data/local/tmp/mobile_bench
echo "device: $(adb shell getprop ro.product.model) / $(adb shell getprop ro.soc.model 2>/dev/null)"
for threads in 1 2 4 8; do
  adb shell "RAYON_NUM_THREADS=$threads /data/local/tmp/mobile_bench prove"
done
adb shell "/data/local/tmp/mobile_bench sign"
adb shell rm /data/local/tmp/mobile_bench
