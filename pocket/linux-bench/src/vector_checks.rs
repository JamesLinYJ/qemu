// SPDX-License-Identifier: MIT
use anyhow::{Result, ensure};
use std::arch::asm;

/// Check equal-boundary cases of the actual AArch64 scalar SIMD instructions.
/// Keep inputs in registers so the compiler cannot constant-fold the tests.
pub fn run() -> Result<()> {
    for input in [-2_i64, -1, 0, 1, 2, i64::MIN, i64::MAX] {
        let (le, lt): (u64, u64);
        unsafe {
            asm!(
                "fmov d0, {input}",
                "cmle d1, d0, #0",
                "cmlt d2, d0, #0",
                "fmov {le}, d1",
                "fmov {lt}, d2",
                input = in(reg) input,
                le = lateout(reg) le,
                lt = lateout(reg) lt,
                out("v0") _, out("v1") _, out("v2") _,
                options(nomem, nostack, preserves_flags),
            );
        }
        ensure!(
            le == if input <= 0 { u64::MAX } else { 0 },
            "CMLE zero boundary: input={input}, actual={le:#x}"
        );
        ensure!(
            lt == if input < 0 { u64::MAX } else { 0 },
            "CMLT zero boundary: input={input}, actual={lt:#x}"
        );
    }
    println!("vector-zero-boundaries-ok");
    Ok(())
}
