#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Exercise translated loads/stores, page boundaries and protection faults."""
import ctypes
import mmap
import os
import random
import resource
import signal

resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
page = mmap.PAGESIZE
memory = mmap.mmap(-1, page * 2, prot=mmap.PROT_READ | mmap.PROT_WRITE)
base = ctypes.addressof(ctypes.c_char.from_buffer(memory))
rng = random.Random(42)
for size, kind in ((1, ctypes.c_uint8), (2, ctypes.c_uint16),
                   (4, ctypes.c_uint32), (8, ctypes.c_uint64)):
    for offset in (0, 1, 7, page - size, page - size + 1, page - 1, page + 3):
        value = rng.getrandbits(size * 8)
        kind.from_address(base + offset).value = value
        assert memory[offset:offset + size] == value.to_bytes(size, "little")
        assert kind.from_address(base + offset).value == value
        memory[offset:offset + size] = bytes(range(size))
        assert kind.from_address(base + offset).value == int.from_bytes(bytes(range(size)), "little")

libc = ctypes.CDLL(None, use_errno=True)
libc.mprotect.argtypes = (ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int)
libc.mprotect.restype = ctypes.c_int

def must_fault(protection, offset, write):
    child = os.fork()
    if child == 0:
        signal.signal(signal.SIGSEGV, signal.SIG_DFL)
        if libc.mprotect(base + page, page, protection) != 0:
            os._exit(2)
        cell = ctypes.c_uint64.from_address(base + offset)
        if write:
            cell.value = 42
        else:
            _ = cell.value
        os._exit(3)
    _, status = os.waitpid(child, 0)
    assert os.WIFSIGNALED(status), (protection, offset, write, status)
    assert os.WTERMSIG(status) == signal.SIGSEGV, status

must_fault(0, page, False)
must_fault(0, page - 4, False)
must_fault(0, page - 4, True)
must_fault(mmap.PROT_READ, page, True)
print("memory-widths-unaligned-cross-page-protection-ok", flush=True)
