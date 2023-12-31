// SPDX-FileCopyrightText: Copyright 2022 EDF (Électricité de France S.A.)
// SPDX-License-Identifier: BSD-3-Clause
// See README for all details on copyright, authorship and license.
//! Dynamic memory management configuration
//!
//! While this project generally avoids dynamic memory management, dcaf and coset do depend on it
//! through ciborium.

extern crate alloc;

static mut HEAP: [u8; 4096] = [0; 4096]; // 512 doesn't suffice even for a minimal token
                                         // response, but we won't change dcaf and coset over
                                         // night. More than 2048 needed when also doing the
                                         // access token decryption.
#[global_allocator]
static ALLOCATOR: embedded_alloc::Heap = embedded_alloc::Heap::empty();

/// Start the global allocator
///
/// Use as late as possible during startup to ensure that earlier allocations fail (we don't want
/// accidental dependencies).
pub unsafe fn init() {
    ALLOCATOR.init(&mut HEAP as *mut _ as usize, core::mem::size_of_val(&HEAP));
}
