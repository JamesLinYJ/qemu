#!/bin/bash
# SPDX-License-Identifier: MIT
# Build the ext4 acceptance disk natively, without extracting Linux metadata
# into APFS. mke2fs must be built with libarchive support.
set -euo pipefail
export LC_ALL=en_US.UTF-8
[[ $# -eq 4 ]] || { echo 'usage: make-test-disk-macos.sh ROOT_TAR MODULES_TAR OUTPUT_DIR MKE2FS' >&2; exit 2; }
root_archive=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
module_archive=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
test ! -e "$3"
mkdir -p "$3"
out=$(cd "$3" && pwd)
mkfs=$4
pocket_dir=$(cd "$(dirname "$0")" && pwd)
case ${POCKET_GUEST_TEST:-acceptance} in
    acceptance) guest_test=guest-acceptance.sh ;;
    performance) guest_test=guest-performance.sh ;;
    shell|regression)
        guest_test=guest-shell-performance.sh
        [[ $POCKET_GUEST_TEST != regression ]] || guest_test=guest-execution-regression.sh
        test -f "${POCKET_LINUX_BENCH_BINARY:?Set the cross-built Linux ARM64 benchmark path.}"
        ;;
    *) echo 'POCKET_GUEST_TEST must be acceptance, performance, shell or regression.' >&2; exit 2 ;;
esac
overlay="$out/overlay"
mkdir -p "$overlay/etc/systemd/system/multi-user.target.wants" "$overlay/usr/local/lib/pocket"
cp "$pocket_dir/$guest_test" "$overlay/usr/local/lib/pocket/guest-acceptance.sh"
cp "$pocket_dir/guest-memory-check.py" "$pocket_dir/guest-atomics-check.cjs" "$overlay/usr/local/lib/pocket/"
if [[ $guest_test == guest-shell-performance.sh || $guest_test == guest-execution-regression.sh ]]; then
    mkdir -p "$overlay/usr/local/libexec"
    install -m 755 "$POCKET_LINUX_BENCH_BINARY" "$overlay/usr/local/libexec/pocket-linux-bench"
fi
chmod 755 "$overlay/usr/local/lib/pocket/guest-acceptance.sh"
ln -s /run/systemd/resolve/stub-resolv.conf "$overlay/etc/resolv.conf"
printf 'pocketagent\n' > "$overlay/etc/hostname"
printf '127.0.0.1 localhost\n127.0.1.1 pocketagent\n::1 localhost ip6-localhost\n' > "$overlay/etc/hosts"
touch "$overlay/etc/fstab"
cat > "$overlay/etc/systemd/system/pocket-acceptance.service" <<'EOF'
[Unit]
Description=Pocket Agent guest component acceptance
Wants=network-online.target
After=network-online.target
[Service]
Type=oneshot
Environment=CI=1 NPM_CONFIG_PROGRESS=false NPM_CONFIG_UPDATE_NOTIFIER=false
ExecStart=/usr/local/lib/pocket/guest-acceptance.sh
TimeoutStartSec=10min
StandardOutput=tty
StandardError=tty
TTYPath=/dev/ttyAMA0
[Install]
WantedBy=multi-user.target
EOF
ln -s ../pocket-acceptance.service "$overlay/etc/systemd/system/multi-user.target.wants/pocket-acceptance.service"
# Remap module paths for Debian's merged /usr, keeping /lib a symlink.
tar -cf "$out/modules.tar" --exclude='*/build' --exclude='*/source' \
    -s '|^lib$|usr/lib|' -s '|^lib/|usr/lib/|' "@$module_archive"
tar --uid 0 --gid 0 -cf "$out/overlay.tar" -C "$overlay" .
tar -cf "$out/combined.tar" --exclude=etc/resolv.conf --exclude=etc/fstab \
    --exclude=etc/hostname --exclude=etc/hosts --exclude=.dockerenv --exclude=run/.containerenv "@$root_archive"
tar -rf "$out/combined.tar" "@$out/modules.tar" "@$out/overlay.tar"
mkfile -n 8g "$out/debian-test.ext4"
"$mkfs" -F -t ext4 -b 4096 -L pocket-root -d "$out/combined.tar" "$out/debian-test.ext4"
shasum -a 256 "$out/debian-test.ext4" > "$out/debian-test.ext4.sha256"
