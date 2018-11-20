#![no_std]
#![feature(core_intrinsics)]

/// A Rust port of libsel4twinkle allocator.
///
/// https://github.com/smaccm/libsel4twinkle
///
/// NOTE this is not complete, and is more of an experiment than anything
///
/// TODO docs and such
extern crate sel4_sys;

use sel4_sys::{seL4_CPtr, seL4_Word};

mod allocator;
mod cspacepath;
mod first_stage_allocator;
mod init_cap;
mod io_map;
mod object_allocator;
pub mod object_type;
mod pmem;
mod vka;
pub mod vka_object;
mod vspace;

pub use init_cap::{InitCap, NUM_INITIAL_CAPS};
pub use pmem::{DMACacheOp, PMem};

pub const MIN_UNTYPED_SIZE: usize = 4;
pub const MAX_UNTYPED_SIZE: usize = 32;

// TODO - pull from configs
pub const MAX_UNTYPED_ITEMS: usize = 256;

pub const VKA_NO_PADDR: seL4_Word = 0;

const VSPACE_START: seL4_Word = 0x1000_0000;

pub const PAGE_BITS_4K: seL4_Word = 12;
pub const PAGE_SIZE_4K: seL4_Word = 1 << PAGE_BITS_4K;

// TODO - should be derived from libsel4-sys?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    ResourceExhausted,
    InvalidAddress,
    Other,
}

#[derive(Clone, Debug)]
pub struct UntypedItem {
    cap: seL4_CPtr,
    size_bits: usize,
    paddr: seL4_Word,
    is_device: bool,
}

#[derive(Clone, Debug)]
pub struct CapRange {
    first: usize,
    count: usize,
}

#[derive(Clone, Debug)]
struct InitUntypedItem {
    item: UntypedItem,
    is_free: bool,
}

pub struct Allocator {
    /// Root page directory for our vspace
    page_directory: seL4_CPtr,
    page_table: seL4_CPtr,
    last_allocated: seL4_Word,

    /// CNode we allocate from
    root_cnode: seL4_CPtr,
    root_cnode_depth: seL4_CPtr,
    root_cnode_offset: seL4_CPtr,

    /// Range of free slots in the root cnode
    cslots: CapRange,

    /// Number fo slots we've used
    num_slots_used: usize,

    /// Initial memory items
    num_init_untyped_items: usize,
    init_untyped_items: [InitUntypedItem; MAX_UNTYPED_ITEMS],

    /// Untyped memory items we have created
    untyped_items: [CapRange; (MAX_UNTYPED_SIZE - MIN_UNTYPED_SIZE) + 1],
}
