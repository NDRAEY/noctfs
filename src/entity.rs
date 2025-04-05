use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
use arrayref::array_ref;
use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    struct EntityFlags: u32 {
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

pub struct Entity {
    name: String,
    size: u64,
    start_block: u64,
    flags: EntityFlags,
    vendor_data_size: u32,
}

impl Entity {
    pub fn file(name: String, size: usize, start_block: usize) -> Self {
        Self {
            name,
            size: size as _,
            start_block: start_block as u64,
            flags: EntityFlags::empty(),
            vendor_data_size: 0,
        }
    }

    pub fn directory<T: ToString>(name: T, size: usize, start_block: u64) -> Self {
        Self {
            name: name.to_string(),
            size: size as _,
            start_block: start_block,
            flags: EntityFlags::DIRECTORY,
            vendor_data_size: 0,
        }
    }

    pub fn as_raw(&self) -> Box<[u8]> {
        let mut data: Vec<u8> = Vec::new();

        let r_namesize = (self.name.len() as u32).to_le_bytes();
        let r_name = self.name.as_bytes();
        let r_size = self.size.to_le_bytes();
        let r_offset = self.start_block.to_le_bytes();
        let r_flags = self.flags.bits().to_le_bytes();
        let r_vendor_data_size = self.vendor_data_size.to_le_bytes();

        data.extend_from_slice(&r_namesize);
        data.extend_from_slice(r_name);
        data.extend_from_slice(&r_size);
        data.extend_from_slice(&r_offset);
        data.extend_from_slice(&r_flags);
        data.extend_from_slice(&r_vendor_data_size);

        let data_len = (data.len() as u32).to_le_bytes();

        let mut new_data: Vec<u8> = Vec::new();
        new_data.extend_from_slice(&data_len);
        new_data.extend_from_slice(&data);

        new_data.into_boxed_slice()
    }

    pub fn from_raw(data: &[u8]) -> Self {
        let (_, rest) = data.split_at(4);    // Skip entity header size
        let (namesize_bytes, rest) = rest.split_at(4);

        let namesize = u32::from_le_bytes(*array_ref![namesize_bytes, 0, 4]) as usize;

        let (name, rest) = rest.split_at(namesize);
        let name = String::from_utf8_lossy(name).into_owned();

        let (size_bytes, rest) = rest.split_at(8);
        let (offset_bytes, rest) = rest.split_at(8);
        let (flags_bytes, rest) = rest.split_at(4);
        let (vendor_data_size_bytes, _) = rest.split_at(4);

        let size = u64::from_le_bytes(*array_ref![size_bytes, 0, 8]);
        let offset = u64::from_le_bytes(*array_ref![offset_bytes, 0, 8]);
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
