#!/bin/bash
# SPDX-License-Identifier: MIT
# Run only in disposable validation images.
set -euo pipefail
finish() {
    result=$?
    if [[ $result -ne 0 ]]; then echo "POCKET_ACCEPTANCE_FAIL:$result"; fi
    sync
    systemctl --no-block poweroff
}
trap finish EXIT
export LC_ALL=C.UTF-8
/usr/local/libexec/pocket-linux-bench --check-vectors
printf 'pocket needle\n' > /tmp/pocket-rg-regression.txt
for round in {1..50}; do
    output=$(rg --color never --no-heading --no-line-number needle /tmp/pocket-rg-regression.txt)
    [[ $output == 'pocket needle' ]]
done
rg --type-list > /tmp/pocket-rg-types.txt
grep -q '^rust:' /tmp/pocket-rg-types.txt
echo ripgrep-search-and-types-ok
/usr/local/libexec/pocket-linux-bench
echo POCKET_ACCEPTANCE_PASS
sleep 1
