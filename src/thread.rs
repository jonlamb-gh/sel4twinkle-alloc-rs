use super::{Allocator, Error};
use core::mem;
use sel4_sys::*;
use vka_object::VkaObject;

pub struct Thread {
    // ipc_frame_object: VkaObject,
    // seL4_IPCBuffer *ipc_buffer,
    // ipc_buffer_vaddr: seL4_Word,
    tcb_obj: VkaObject,
    //ipc_ep_obj: VkaObject,
    pub ipc_ep_cap: seL4_CPtr,
    stack_top: seL4_Word,
}

impl Allocator {
    pub fn create_thread(
        &mut self,
        fault_ep_cap: seL4_CPtr,
        fault_ep_badge: seL4_Word,
        ipc_ep_badge: seL4_Word,
        stack_size_pages: usize,
    ) -> Result<Thread, Error> {
        let tcb_obj = self.vka_alloc_tcb()?;
        let tcb_cap = tcb_obj.cptr;

        let pd_cap = self.vspace_get_root();
        let cspace_cap = self.root_cnode;

        // Create a IPC buffer and page directory capability for it
        let mut ipc_pd_cap: seL4_CPtr = 0;
        let ipc_buffer_vaddr = self.vspace_new_ipc_buffer(Some(&mut ipc_pd_cap))?;

        // Set the IPC buffer's virtual address in a field of the IPC buffer
        let ipc_buffer: *mut seL4_IPCBuffer = ipc_buffer_vaddr as _;
        unsafe { (*ipc_buffer).userData = ipc_buffer_vaddr };

        // Allocate a cspace slot for the badged fault endpoint used by the thread
        let badged_fault_ep_cap = self.vka_cspace_alloc()?;

        // Create/mint a badged fault endpoint for the thread
        let err = unsafe {
            seL4_CNode_Mint(
                cspace_cap,
                badged_fault_ep_cap,
                seL4_WordBits as _,
                cspace_cap,
                fault_ep_cap,
                seL4_WordBits as _,
                seL4_CapRights_new(1, 1, 1),
                fault_ep_badge,
            )
        };
        if err != 0 {
            return Err(Error::Other);
        }

        // Create an IPC endpoint
        let ipc_ep_obj = self.vka_alloc_endpoint()?;

        // Allocate a cspace slot for the badged IPC endpoint
        let badged_ipc_ep_cap = self.vka_cspace_alloc()?;

        // Create/mint a badged IPC endpoint for the thread
        let err = unsafe {
            seL4_CNode_Mint(
                cspace_cap,
                badged_ipc_ep_cap,
                seL4_WordBits as _,
                cspace_cap,
                ipc_ep_obj.cptr,
                seL4_WordBits as _,
                seL4_CapRights_new(1, 1, 1),
                ipc_ep_badge,
            )
        };
        if err != 0 {
            return Err(Error::Other);
        }

        let err = unsafe {
            seL4_TCB_Configure(
                tcb_cap,
                badged_fault_ep_cap,
                cspace_cap.into(),
                seL4_NilData.into(),
                pd_cap.into(),
                seL4_NilData.into(),
                ipc_buffer_vaddr,
                ipc_pd_cap,
            )
        };
        if err != 0 {
            return Err(Error::Other);
        }

        // Allocate a new stack for the thread from the vspace
        let stack_top = self.vspace_new_sized_stack(stack_size_pages)?;

        let stack_size = stack_size_pages * (1 << seL4_PageBits);
        let stack_alignment_requirement: usize = (seL4_WordBits as usize / 8) * 2;

        assert!(stack_size >= 512, "Thread stack size is too small");
        assert!(
            stack_size % stack_alignment_requirement == 0,
            "Thread stack is not properly aligned to a {} byte boundary",
            stack_alignment_requirement
        );

        assert!(
            (stack_top as usize) % stack_alignment_requirement == 0,
            "Thread stack is not properly aligned to a {} byte boundary",
            stack_alignment_requirement
        );

        Ok(Thread {
            tcb_obj,
            //ipc_ep_obj,
            ipc_ep_cap: badged_ipc_ep_cap,
            stack_top,
        })
    }
}

impl Thread {
    pub fn configure_context(
        &mut self,
        run_fn: seL4_Word,
        arg0: Option<seL4_Word>,
        arg1: Option<seL4_Word>,
        arg2: Option<seL4_Word>,
    ) -> Result<(), Error> {
        let mut regs: seL4_UserContext = unsafe { mem::zeroed() };

        // Registers pc, sp, cpsr are always used
        let mut context_word_size = 3;

        // probably should just write the whole thing regardless
        #[cfg(all(target_arch = "arm", target_os = "sel4", target_env = "fel4"))]
        {
            if let Some(r0) = arg0 {
                regs.r0 = r0;
                context_word_size = 4;

                if let Some(r1) = arg1 {
                    regs.r1 = r1;
                    context_word_size = 5;

                    if let Some(r2) = arg2 {
                        regs.r2 = r2;
                        context_word_size = 11;
                    }
                }
            }
        }
        #[cfg(all(
            target_arch = "aarch64",
            target_os = "sel4",
            target_env = "fel4"
        ))]
        {
            if let Some(r0) = arg0 {
                regs.x0 = r0;
                context_word_size = 4;

                if let Some(r1) = arg1 {
                    regs.x1 = r1;
                    context_word_size = 5;

                    if let Some(r2) = arg2 {
                        regs.x2 = r2;
                        context_word_size = 6;
                    }
                }
            }
        }

        // Set the pc and sp
        regs.pc = run_fn as seL4_Word;
        regs.sp = self.stack_top;

        let err = unsafe {
            seL4_TCB_WriteRegisters(self.tcb_obj.cptr, 0, 0, context_word_size, &mut regs)
        };
        if err != 0 {
            return Err(Error::Other);
        }

        Ok(())
    }

    pub fn start(&mut self, authority_cap: seL4_CPtr) -> Result<(), Error> {
        let err = unsafe {
            seL4_TCB_SetPriority(
                self.tcb_obj.cptr,
                authority_cap,
                priorityConstants_seL4_MaxPrio as _,
            )
        };
        if err != 0 {
            return Err(Error::Other);
        }

        let err = unsafe { seL4_TCB_Resume(self.tcb_obj.cptr) };
        if err != 0 {
            return Err(Error::Other);
        }

        Ok(())
    }
}
