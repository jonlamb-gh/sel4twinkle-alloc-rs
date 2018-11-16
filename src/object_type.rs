// TODO - move item_size logic here

use sel4_sys::seL4_Word;

//pub const api_object_seL4_NonArchObjectTypeCount: api_object = 5;
//pub const _mode_object_seL4_ModeObjectTypeCount: _mode_object = 8;
//pub const _object_seL4_ObjectTypeCount: _object = 12;

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ObjectType {
    UntypedObject,
    TCBObject,
    EndpointObject,
    NotificationObject,
    CapTableObject,
    ARM_HugePageObject,
    ARM_PageUpperDirectoryObject,
    ARM_PageGlobalDirectoryObject,
    ARM_SmallPageObject,
    ARM_LargePageObject,
    ARM_PageTableObject,
    ARM_PageDirectoryObject,
}

impl From<ObjectType> for seL4_Word {
    fn from(obj_type: ObjectType) -> seL4_Word {
        match obj_type {
            ObjectType::UntypedObject => 0,
            ObjectType::TCBObject => 1,
            ObjectType::EndpointObject => 2,
            ObjectType::NotificationObject => 3,
            ObjectType::CapTableObject => 4,
            ObjectType::ARM_HugePageObject => 5,
            ObjectType::ARM_PageUpperDirectoryObject => 6,
            ObjectType::ARM_PageGlobalDirectoryObject => 7,
            ObjectType::ARM_SmallPageObject => 8,
            ObjectType::ARM_LargePageObject => 9,
            ObjectType::ARM_PageTableObject => 10,
            ObjectType::ARM_PageDirectoryObject => 11,
        }
    }
}
