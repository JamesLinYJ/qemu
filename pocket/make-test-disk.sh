#!/bin/bash
# SPDX-License-Identifier: MIT
# Run in the Linux builder with an exported Debian image in /out/rootfs.tar.
set -euo pipefail
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
test ! -f /out/debian-test.ext4
mkdir -p /rootfs
tar --numeric-owner -xf /out/rootfs.tar -C /rootfs
tar --keep-directory-symlink --numeric-owner -xzf /kernel/modules.tar.gz -C /rootfs
ln -snf /run/systemd/resolve/stub-resolv.conf /rootfs/etc/resolv.conf
touch /rootfs/etc/fstab
install -D -m 755 "/input/$guest_test" /rootfs/usr/local/lib/pocket/guest-acceptance.sh
install -D -m 644 /input/guest-memory-check.py /rootfs/usr/local/lib/pocket/guest-memory-check.py
install -D -m 644 /input/guest-atomics-check.cjs /rootfs/usr/local/lib/pocket/guest-atomics-check.cjs
if [[ $guest_test == guest-shell-performance.sh || $guest_test == guest-execution-regression.sh ]]; then
    install -D -m 755 "$POCKET_LINUX_BENCH_BINARY" /rootfs/usr/local/libexec/pocket-linux-bench
fi
cat > /rootfs/etc/systemd/system/pocket-acceptance.service <<'EOF'
[Unit]
Description=Pocket Agent guest component acceptance
Wants=network-online.target
After=network-online.target
[Service]
Type=oneshot
ExecStart=/usr/local/lib/pocket/guest-acceptance.sh
TimeoutStartSec=10min
StandardOutput=tty
StandardError=tty
TTYPath=/dev/ttyAMA0
[Install]
WantedBy=multi-user.target
EOF
chroot /rootfs systemctl --root=/ enable pocket-acceptance.service
chroot /rootfs dpkg-query -W > /out/runtime-packages.tsv
stat -c '%a %u:%g %n' /rootfs/tmp /rootfs/var/tmp /rootfs/usr/bin/su /rootfs/etc/shadow > /out/metadata.txt
truncate -s 8G /out/debian-test.ext4
mkfs.ext4 -F -L pocket-root -d /rootfs /out/debian-test.ext4
sha256sum /out/debian-test.ext4 > /out/debian-test.ext4.sha256
