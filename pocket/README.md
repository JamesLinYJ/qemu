# Pocket Agent QEMU runtime

The baseline is official QEMU `v11.1.2`, commit
`4fc49f46dc95d4a27de2509e7fceb2931e91faeb`, from the
[official GitHub mirror](https://github.com/qemu/qemu). The canonical upstream
is <https://gitlab.com/qemu-project/qemu.git>. See the
[official release page](https://www.qemu.org/download/). Keep upstream imports
separate from Pocket Agent changes.

## Build and trimming

```sh
POCKET_BUILD_PYTHON=/path/to/python3.11-or-newer bash pocket/build-macos.sh
cargo build --release --manifest-path pocket/runtime/Cargo.toml
pocket/runtime/target/release/pocket-vm /tmp/pocket-qmp-test
pocket/runtime/target/release/pocket-vm --resources /tmp/pocket-disk-test
```

The Apple Silicon macOS validation build needs Clang, Meson, Ninja, pkg-config,
GLib, libffi, libslirp and libfdt. It selects only aarch64-softmmu, the virt
machine and the required virtio transports. No display, audio or unrelated
boards are required. Networking, DNS, TLS, SSH, project servers, disk read/write,
QCOW2, snapshots, random data, terminal/PTY and process control remain required
acceptance cases. Linux stays a standard ARM64 defconfig with additive guest
drivers and retained modules; Debian 13 keeps systemd and development tools.

The interpreter profile uses upstream TCI, with HVF and host JIT disabled.
No performance result from another source baseline proves this build's speed.
Treat macOS components, iOS simulator checks and iPhone results as separate
evidence. A successful Mac build does not qualify as an iOS port.

The optional Meson setting `-Dpocket_system_library=true` builds an in-process
system library in addition to the ordinary upstream executable. It reuses the
existing qemu_init/main_loop/cleanup implementation without the desktop main()
entry point. Rust declarations are generated from actual QEMU headers with
bindgen; there is no second handwritten C ABI.

QEMU's process-global state allows one lifecycle in the validation host. An
application must keep a VM alive across commands, own its worker thread and
transport, and handle shutdown/error paths before deployment. The validation
host is not yet a production iOS library or restart supervisor.

## Validation

Pass a kernel Image and a disposable ext4 disk after the state directory to run
Linux acceptance. guest-acceptance.sh covers networking, package installation,
Python/Node/npm, threads, memory protection and shared atomics. The separate
QCOW2 resource test writes A, saves a snapshot, writes B, restores the snapshot
and reads back A through QEMU's NBD export. It uses the host qemu-io client
while the guest is paused.

The offline guest-performance.sh workload compares separate clones of the same
stopped disk. Run variants serially and record CPU count, RAM, translation cache
and build flags. The Rust host records CPU time, RSS, physical footprint, block
accounting and TCG statistics. x-query-jit is QMP's statistics command name even
in an interpreter build.

linux-bench is a Rust guest-only latency harness. Cross-build it on macOS with:

```sh
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld cargo build --release --locked --target aarch64-unknown-linux-musl --manifest-path pocket/linux-bench/Cargo.toml
```

It measures fresh Bash processes, pipelines, ripgrep, Python and a persistent
PTY. It validates output and reports first-output/completion P50/P95. These
timings exclude Codex and host-to-guest transport. Independent agent commands
retain their own environment/cwd; only an existing command or PTY is continued
for interactive input.

Set POCKET_GUEST_TEST=shell and POCKET_LINUX_BENCH_BINARY when assembling a
disposable latency image. guest-execution-regression.sh also checks SIMD
comparison boundaries and repeated real ripgrep searches/type discovery. Do
not include intentional fault tests or shutdown services in release images.
Run a complete agent task separately before switching the app backend.

make-test-disk-macos.sh assembles Linux metadata directly into ext4 using BSD
tar and mke2fs with libarchive. Owners, setuid/sticky modes and Linux paths must
not be normalized through APFS extraction.

## Licensing

Original QEMU licenses and notices remain in force; see [LICENSE](../LICENSE)
and [COPYING](../COPYING). New Rust support code is MIT where marked by SPDX.
It does not relicense QEMU. Before distributing an in-process combination with
the Apache-2.0 agent core, audit the selected objects and dependencies for
compatible terms. FFI and dynamic linking do not decide that question.
