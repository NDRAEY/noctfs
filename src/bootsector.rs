use alloc::boxed::Box;

use crate::FILESYSTEM_CODENAME;

const BOOTCODE: &[u8; 512] = include_bytes!("../static/bootcode.bin");

#[derive(Debug)]
#[repr(packed)]
pub struct BootSector {
    pub(crate) filesystem_codename: [u8; 8],
    pub(crate) sector_size: u16,
    pub(crate) block_size: u32,
    pub(crate) block_map_count: u32,
    pub(crate) first_root_entity_lba: u64,
}

impl BootSector {
    pub fn with_data(device_size: usize, sector_size: u16, block_size: u32) -> Self {
        let block_map_count = device_size / block_size as usize;
        let first_root_entry = sector_size as usize + block_map_count;

        let mut codename: [u8; 8] = [0; 8];
        codename.copy_from_slice(FILESYSTEM_CODENAME);

        Self {
            filesystem_codename: codename,
            sector_size,
            block_size,
            block_map_count: block_map_count as u32,
            first_root_entity_lba: (first_root_entry / sector_size as usize) as u64,
        }
    }

    pub fn as_raw(&self) -> Box<[u8]> {
        let mut sector: [u8; 512] = *BOOTCODE;
        let self_size = core::mem::size_of::<Self>();
        let raw_ptr = self as *const Self;
        let raw_data = unsafe { core::slice::from_raw_parts(raw_ptr as *const u8, self_size) };

        sector[3..self_size + 3].copy_from_slice(raw_data);

        Box::new(sector)
    }

    pub fn from_raw(data: &[u8; 512]) -> Self {
        let raw_ptr = data[3..].as_ptr() as *const Self;

        unsafe { raw_ptr.read() }
    }
}
