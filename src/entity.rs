use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
use arrayref::array_ref;
use bitflags::bitflags;

use crate::{BlockAddress, BLOCK_ADDRESS_SIZE};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct EntityFlags: u32 {
        const DIRECTORY = (1 << 0);
    }
}

///  [0..4]           (4 bytes) - Entity header size
///  [4..8]           (4 bytes) - Entity name length
///  [8..8+n]         (n bytes) - Entity name in UTF-8
///  [8+n..8+n+8]     (8 bytes) - Entity size (file count if entity is directory)
///  [8+n+8..8+n+16]  (8 bytes) - Data offset (block number)
///  [8+n+16..8+n+20] (4 bytes) - Flags
///  [8+n+20..8+n+24] (4 bytes) - Vendor data size

#[derive(Debug, Clone)]
pub struct Entity {
    pub name: String,
    pub size: u64,
    pub start_block: BlockAddress,
    pub flags: EntityFlags,
    pub vendor_data_size: u32,
}

impl Entity {
    pub fn file<T: ToString>(name: T, size: usize, start_block: BlockAddress) -> Self {
        Self {
            name: name.to_string(),
            size: size as _,
            start_block: start_block as u64,
            flags: EntityFlags::empty(),
            vendor_data_size: 0,
        }
    }

    pub fn directory<T: ToString>(name: T, size: usize, start_block: BlockAddress) -> Self {
        Self {
            name: name.to_string(),
            size: size as _,
            start_block: start_block,
            flags: EntityFlags::DIRECTORY,
            vendor_data_size: 0,
        }
    }

    // Header size field NOT included!
    pub fn header_size(&self) -> u32 {
        (4 + self.name.len() + 8 + 8 + 4 + 4 + self.vendor_data_size as usize) as u32
    }

    pub fn fact_size(&self) -> u32 {
        self.header_size() + 4
    }

    pub fn as_raw(&self) -> Box<[u8]> {
        let mut data: Vec<u8> = Vec::new();

        let r_header_size = self.header_size().to_le_bytes();
        let r_namesize = (self.name.len() as u32).to_le_bytes();
        let r_name = self.name.as_bytes();
        let r_size = self.size.to_le_bytes();
        let r_offset = self.start_block.to_le_bytes();
        let r_flags = self.flags.bits().to_le_bytes();
        let r_vendor_data_size = self.vendor_data_size.to_le_bytes();

        data.extend_from_slice(&r_header_size);
        data.extend_from_slice(&r_namesize);
        data.extend_from_slice(r_name);
        data.extend_from_slice(&r_size);
        data.extend_from_slice(&r_offset);
        data.extend_from_slice(&r_flags);
        data.extend_from_slice(&r_vendor_data_size);

        data.into_boxed_slice()
    }

    pub fn from_raw(data: &[u8]) -> Self {
        let (_, rest) = data.split_at(4);    // Skip entity header size
        let (namesize_bytes, rest) = rest.split_at(4);

        let namesize = u32::from_le_bytes(*array_ref![namesize_bytes, 0, 4]) as usize;

        let (name, rest) = rest.split_at(namesize);
        let name = String::from_utf8_lossy(name).into_owned();

        let (size_bytes, rest) = rest.split_at(8);
        let (offset_bytes, rest) = rest.split_at(BLOCK_ADDRESS_SIZE);
        let (flags_bytes, rest) = rest.split_at(4);
        let (vendor_data_size_bytes, _) = rest.split_at(4);

        let size = u64::from_le_bytes(*array_ref![size_bytes, 0, 8]);
        let offset = u64::from_le_bytes(*array_ref![offset_bytes, 0, BLOCK_ADDRESS_SIZE]);
        let flags =
            EntityFlags::from_bits(u32::from_le_bytes(*array_ref![flags_bytes, 0, 4])).unwrap();
        let vendor_data_size = u32::from_le_bytes(*array_ref![vendor_data_size_bytes, 0, 4]);

        Self {
            name,
            size,
            start_block: offset,
            flags: flags,
            vendor_data_size: vendor_data_size,
        }
    }

    pub fn is_file(&self) -> bool {
        !self.flags.contains(EntityFlags::DIRECTORY)
    }

    pub fn is_directory(&self) -> bool {
        self.flags.contains(EntityFlags::DIRECTORY)
    }
}
