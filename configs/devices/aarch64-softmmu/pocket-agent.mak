# SPDX-License-Identifier: GPL-2.0-or-later
# ARM64 virt machine and required transports. Kconfig supplies dependencies.
CONFIG_ARM_VIRT=y
CONFIG_VIRTIO_PCI=y
CONFIG_PCIE_PORT=y
CONFIG_VIRTIO_BALLOON=y
CONFIG_VIRTIO_BLK=y
CONFIG_VIRTIO_NET=y
CONFIG_VIRTIO_SERIAL=y
CONFIG_VIRTIO_RNG=y
