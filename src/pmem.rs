// TODO - this allocator doesn't book keep untyped splits yet
// will just consume entire untyped with paddr to beginning until then

use super::{Allocator, Error, PAGE_BITS_4K, PAGE_SIZE_4K};
use sel4_sys::*;

/// Physical memory
#[derive(Debug, Copy, Clone)]
pub struct PMem {
    pub vaddr: seL4_Word,
    pub paddr: seL4_Word,
}

impl Allocator {
    pub fn pmem_new_page(&mut self, cap: Option<&mut seL4_CPtr>) -> Result<PMem, Error> {
        let vaddr = self.last_allocated;

        let obj = self.vka_alloc_frame(PAGE_BITS_4K as _)?;

        self.last_allocated += PAGE_SIZE_4K;

        let result: seL4_ARM_Page_GetAddress_t = unsafe { seL4_ARM_Page_GetAddress(obj.cptr) };
        if result.error != 0 {
            return Err(Error::Other);
        }

        self.map_page(
            obj.cptr,
            vaddr,
            // rights,
            unsafe { seL4_CapRights_new(1, 1, 1) },
            seL4_ARM_VMAttributes_seL4_ARM_Default_VMAttributes,
        )?;

        // provide cap to the first frame
        if let Some(cap) = cap {
            *cap = obj.cptr;
        }

        Ok(PMem {
            vaddr,
            paddr: result.paddr,
        })
    }
}
