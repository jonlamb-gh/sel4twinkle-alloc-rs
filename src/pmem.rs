use super::{Allocator, Error, PAGE_BITS_4K, PAGE_SIZE_4K};
use core::intrinsics;
use object_type::ObjectType;
use sel4_sys::*;
use vka_object::VkaObject;

/// Physical memory
#[derive(Debug, Copy, Clone)]
pub struct PMem {
    pub vaddr: seL4_Word,
    pub paddr: seL4_Word,
    // TODO - pd_cap and/or frame cap?
}

#[derive(Debug, Copy, Clone)]
pub enum DMACacheOp {
    Clean,
    Invalidate,
    CleanInvalidate,
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
            // TODO - not cacheable
            // 0,
            seL4_ARM_VMAttributes_seL4_ARM_Default_VMAttributes,
        )?;

        if let Some(cap) = cap {
            *cap = obj.cptr;
        }

        Ok(PMem {
            vaddr,
            paddr: result.paddr,
        })
    }

    pub fn pmem_new_dma_page(&mut self, cap: Option<&mut seL4_CPtr>) -> Result<PMem, Error> {
        let size_bits = PAGE_BITS_4K;

        let ut: VkaObject = self.vka_alloc_untyped(size_bits as _)?;

        let frame_cap = self.vka_cspace_alloc()?;
        let mut path = self.vka_cspace_make_path(frame_cap);

        let err = unsafe {
            seL4_Untyped_Retype(
                ut.cptr,
                ObjectType::ARM_SmallPageObject.into(),
                size_bits as _,
                path.root,
                path.dest,
                path.dest_depth,
                path.offset,
                1,
            )
        };
        if err != 0 {
            return Err(Error::ResourceExhausted);
        }

        let frame_vaddr = self.vspace_new_pages(
            1,
            PAGE_BITS_4K as _,
            unsafe { seL4_CapRights_new(1, 1, 1) },
            //seL4_ARM_VMAttributes_seL4_ARM_Default_VMAttributes,
            0,
            Some(&mut path.cap_ptr),
        )?;

        let result: seL4_ARM_Page_GetAddress_t = unsafe { seL4_ARM_Page_GetAddress(path.cap_ptr) };
        if result.error != 0 {
            return Err(Error::Other);
        }

        if let Some(cap) = cap {
            *cap = path.cap_ptr;
        }

        Ok(PMem {
            vaddr: frame_vaddr,
            paddr: result.paddr,
        })
    }

    pub fn dma_cache_op(&self, vaddr: seL4_Word, size: usize, op: DMACacheOp) {
        let root = self.vspace_get_root();
        let end = vaddr + size as seL4_Word;
        let mut cur = vaddr;

        while cur < end {
            let mut top = round_up(cur as usize + 1, PAGE_SIZE_4K as _) as seL4_Word;
            if top > end {
                top = end;
            }

            let err = match op {
                DMACacheOp::Clean => unsafe {
                    seL4_ARM_PageGlobalDirectory_Clean_Data(root, cur, top)
                },
                DMACacheOp::Invalidate => unsafe {
                    seL4_ARM_PageGlobalDirectory_Invalidate_Data(root, cur, top)
                },
                DMACacheOp::CleanInvalidate => unsafe {
                    seL4_ARM_PageGlobalDirectory_CleanInvalidate_Data(root, cur, top)
                },
            };
            assert!(err == 0, "DMA ops failed");

            cur = top;
        }
    }

    pub fn pmem_new_pages_at_paddr(
        &mut self,
        ut_paddr: seL4_Word,
        paddr: seL4_Word,
        num_pages: usize,
        cap: Option<&mut seL4_CPtr>,
    ) -> Result<PMem, Error> {
        // Get the base size then round up to the next power of 2 size.
        // This is because untypeds are allocated in powers of 2
        let size = num_pages * PAGE_SIZE_4K as usize;
        let base_size_bits = log_base_2(size);

        let size_bits = if (1 << base_size_bits) != size {
            base_size_bits + 1
        } else {
            base_size_bits
        };

        let ut: VkaObject =
            self.vka_alloc_object_at(ObjectType::UntypedObject, size_bits, ut_paddr)?;

        // TODO - retype/split so we can start at paddr (if paddr != ut_paddr)?
        assert_eq!(ut_paddr, paddr, "Offset from base not impl yet");

        // TODO - use heapless or caller provided?
        let mut caps: [seL4_CPtr; 316] = [0; 316];
        assert!(num_pages <= 316);

        // Allocate all of the frames
        for f in 0..num_pages {
            let cap = self.vka_cspace_alloc()?;
            let path = self.vka_cspace_make_path(cap);

            let err = unsafe {
                seL4_Untyped_Retype(
                    ut.cptr,
                    ObjectType::ARM_SmallPageObject.into(),
                    size_bits as _,
                    path.root,
                    path.dest,
                    path.dest_depth,
                    path.offset,
                    1,
                )
            };
            if err != 0 {
                return Err(Error::ResourceExhausted);
            }

            caps[f] = path.cap_ptr;
        }

        // Base of the reservation
        let base_vaddr = self.last_allocated;

        // Map in all of the pages
        for f in 0..num_pages {
            let frame_vaddr = self.last_allocated;

            self.map_page(
                caps[f],
                frame_vaddr,
                unsafe { seL4_CapRights_new(1, 1, 1) },
                seL4_ARM_VMAttributes_seL4_ARM_Default_VMAttributes,
            )?;

            self.last_allocated += PAGE_SIZE_4K;
        }

        Ok(PMem {
            vaddr: base_vaddr,
            paddr: paddr,
        })
    }
}

fn round_up(val: usize, base: usize) -> usize {
    val + if val % base == 0 {
        0
    } else {
        base - (val % base)
    }
}

fn log_base_2(val: usize) -> usize {
    // sizeof(word) * CHAR_BIT - CLZL(n) - 1
    (8 * 8) - unsafe { intrinsics::ctlz(val as u64) } as usize - 1
}
