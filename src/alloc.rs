// SPDX-FileCopyrightText: Copyright 2022-2024 EDF (Électricité de France S.A.)
// SPDX-License-Identifier: BSD-3-Clause
// See README for all details on copyright, authorship and license.
//! Dynamic memory management configuration
//!
//! While this project generally avoids dynamic memory management, dcaf and coset do depend on it
//! through ciborium.

extern crate alloc;

use embedded_alloc::LlffHeap as Heap;

#[global_allocator]
static ALLOCATOR: Heap = Heap::empty();

/// Start the global allocator
///
/// Use as late as possible during startup to ensure that earlier allocations fail (we don't want
/// accidental dependencies).
pub unsafe fn init() {
    use core::mem::MaybeUninit;

    // 512 doesn't suffice even for a minimal token response, but we won't change dcaf and coset
    // over night. More than 2048 needed when also doing the access token decryption.
    const HEAP_SIZE: usize = 4096;
    static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
    unsafe { ALLOCATOR.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE) }
}
