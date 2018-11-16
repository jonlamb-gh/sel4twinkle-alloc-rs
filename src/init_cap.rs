use sel4_sys::{seL4_NumInitialCaps, seL4_Word};

pub const NUM_INITIAL_CAPS: seL4_Word = seL4_NumInitialCaps as seL4_Word;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum InitCap {
    Null,
    InitThreadTCB,
    InitThreadCNode,
    InitThreadVSpace,
    IRQControl,
    ASIDControl,
    InitThreadASIDPool,
    IOPortControl,
    IOSpace,
    BootInfoFrame,
    InitThreadIPCBuffer,
    Domain,
}

impl From<InitCap> for seL4_Word {
    fn from(cap: InitCap) -> seL4_Word {
        match cap {
            InitCap::Null => 0,
            InitCap::InitThreadTCB => 1,
            InitCap::InitThreadCNode => 2,
            InitCap::InitThreadVSpace => 3,
            InitCap::IRQControl => 4,
            InitCap::ASIDControl => 5,
            InitCap::InitThreadASIDPool => 6,
            InitCap::IOPortControl => 7,
            InitCap::IOSpace => 8,
            InitCap::BootInfoFrame => 9,
            InitCap::InitThreadIPCBuffer => 10,
            InitCap::Domain => 11,
        }
    }
}
