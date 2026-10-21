#!/bin/bash
# SPDX-License-Identifier: MIT
set -euo pipefail

mode=${1:-tci}
[[ $mode == tci ]] || { echo 'usage: pocket/build-macos.sh [tci]' >&2; exit 2; }
[[ $(uname -s) == Darwin && $(uname -m) == arm64 ]] || {
    echo 'This validation build requires an Apple Silicon Mac.' >&2; exit 2;
}
source_dir=$(cd "$(dirname "$0")/.." && pwd)
build_dir="$source_dir/build-pocket-$mode"
python_bin=${POCKET_BUILD_PYTHON:-$(command -v python3)}
"$python_bin" -c 'import sys, tomllib; assert sys.version_info >= (3, 11)'
mkdir -p "$build_dir"
cd "$build_dir"
"$source_dir/configure" \
    --python="$python_bin" \
    --target-list=aarch64-softmmu \
    --with-devices-aarch64=pocket-agent \
    --without-default-devices --without-default-features \
    --enable-system --enable-tcg --enable-tcg-interpreter \
    -Dpocket_system_library=true --enable-slirp --enable-fdt \
    --disable-hvf --disable-werror \
    --audio-drv-list= --disable-docs --disable-tools --disable-guest-agent
ninja -j "${POCKET_BUILD_JOBS:-8}" qemu-system-aarch64 libqemu-aarch64-softmmu.dylib

# Configuration must prove which backend is being measured.
test -f config-host.h
if grep -q '^#define CONFIG_HVF' config-host.h aarch64-softmmu-config-target.h; then
    echo 'Unexpected HVF support in interpreter validation build.' >&2; exit 1
fi
"$build_dir/qemu-system-aarch64" --version
