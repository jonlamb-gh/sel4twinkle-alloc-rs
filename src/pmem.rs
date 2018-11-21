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
        paddr: seL4_Word,
        num_pages: usize,
        cache_attributes: seL4_ARM_VMAttributes,
    ) -> Result<PMem, Error> {
        // Get the base paddr that contains the given start paddr
        let ut_paddr = self.contained_paddr(paddr)?;

        // Get the base size then round up to the next power of 2 size.
        // This is because untypeds are allocated in powers of 2
        let size = num_pages * PAGE_SIZE_4K as usize;
        let base_size_bits = log_base_2(size);

        let size_bits = if (1 << base_size_bits) != size {
            base_size_bits + 1
        } else {
            base_size_bits
        };

        let ut: VkaObject = if ut_paddr != paddr {
            // Desired paddr is not at the start of the untyped,
            // so iteratively retyped it as close to the desired
            // paddr as possible

            // Allocate the whole untyped region
            let untyped_full_size_bits = self.untyped_size_bits(ut_paddr)?;
            let obj: VkaObject = self.vka_alloc_object_at(
                ObjectType::UntypedObject,
                untyped_full_size_bits as _,
                ut_paddr,
            )?;

            let mut remaining: seL4_Word = paddr - ut_paddr;

            // TODO - this only works when things are aligned to powers of 2 for now

            // Retype until we can go no further
            while remaining != 0 {
                // Can only retype in powers of 2
                let skip_size_bits = log_base_2(remaining as _) as seL4_Word;
                assert!(remaining >= 1 << skip_size_bits, "TODO - reduce size bits");

                let cap = self.vka_cspace_alloc()?;
                let path = self.vka_cspace_make_path(cap);

                let err = unsafe {
                    seL4_Untyped_Retype(
                        obj.cptr,
                        ObjectType::UntypedObject.into(),
                        skip_size_bits,
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

                remaining -= 1 << skip_size_bits;
            }

            // Return the untyped now that it's next available
            // cap will at the desired paddr (or as close to it as possible)
            obj
        } else {
            self.vka_alloc_object_at(ObjectType::UntypedObject, size_bits, ut_paddr)?
        };

        // TODO - use heapless or caller provided?
        let mut caps: [seL4_CPtr; 64] = [0; 64];
        assert!(num_pages <= caps.len());

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

        // Sanity check we are starting at the desired paddr
        let first_cap = caps[0];
        let result: seL4_ARM_Page_GetAddress_t = unsafe { seL4_ARM_Page_GetAddress(first_cap) };
        if result.error != 0 {
            return Err(Error::Other);
        }
        let base_paddr = result.paddr;
        assert_eq!(base_paddr, paddr, "Failed to map paddr");

        // Map in all of the pages
        for f in 0..num_pages {
            let frame_vaddr = self.last_allocated;

            self.map_page(
                caps[f],
                frame_vaddr,
                unsafe { seL4_CapRights_new(1, 1, 1) },
                cache_attributes,
            )?;

            self.last_allocated += PAGE_SIZE_4K;
        }

        Ok(PMem {
            vaddr: base_vaddr,
            paddr: paddr,
        })
    }

    /// Returns the base paddr of the untyped region that contains the given
    /// paddr, if any
    pub fn contained_paddr(&self, paddr: seL4_Word) -> Result<seL4_Word, Error> {
        for i in 0..self.num_init_untyped_items {
            let ut_paddr = self.init_untyped_items[i].item.paddr;
            let ut_size = 1 << self.init_untyped_items[i].item.size_bits;
            let ut_paddr_top = ut_paddr + ut_size;

            if (paddr >= ut_paddr) && (paddr <= ut_paddr_top) {
                return Ok(ut_paddr);
            }
        }

        Err(Error::InvalidAddress)
    }

    fn untyped_size_bits(&self, paddr: seL4_Word) -> Result<seL4_Word, Error> {
        for i in 0..self.num_init_untyped_items {
            if paddr == self.init_untyped_items[i].item.paddr {
                return Ok(self.init_untyped_items[i].item.size_bits as seL4_Word);
            }
        }

        Err(Error::InvalidAddress)
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
