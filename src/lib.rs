// #![no_std]

extern crate alloc;

use alloc::vec;
use alloc::{boxed::Box, vec::Vec};
use bootsector::BootSector;
use device::Device;
use no_std_io::io::{
    self, Error,
    SeekFrom::{Current, End, Start},
};

pub mod bootsector;
pub mod device;
pub mod entity;

const ALLOWED_BLOCK_SIZES: &[u32] = &[512, 1024, 2048, 4096, 8192, 16384];
const DEFAULT_BLOCK_SIZE: &u32 = &ALLOWED_BLOCK_SIZES[4]; // 8192
const DEFAULT_SECTOR_SIZE: usize = 512;
const FILESYSTEM_CODENAME: &[u8] = b"NoctFS__";

#[derive(Debug)]
pub enum NoctFSError {
    SignatureNotValid,
    OS(Error),
}

pub struct NoctFS<'dev> {
    bootsector: BootSector,
    device: &'dev mut dyn Device,
}

impl<'dev> NoctFS<'dev> {
    pub fn new(device: &'dev mut dyn Device) -> Result<Self, NoctFSError> {
        let mut bs_data = [0u8; 512];

        device.seek(Start(0)).map_err(|e| NoctFSError::OS(e))?;
        device.read(&mut bs_data).map_err(|e| NoctFSError::OS(e))?;

        let bootsector = BootSector::from_raw(&bs_data);

        if bootsector.filesystem_codename != FILESYSTEM_CODENAME {
            return Err(NoctFSError::SignatureNotValid);
        }

        Ok(Self { bootsector, device })
    }

    pub fn format(device: &'dev mut dyn Device) -> io::Result<()> {
        let size = device.seek(End(0))?;
        device.seek(Start(0))?;

        let mut bootsector = BootSector::with_data(
            size.try_into().unwrap(),
            DEFAULT_SECTOR_SIZE as _,
            *DEFAULT_BLOCK_SIZE as _,
        );

        // Write bootsector

        let sect = bootsector.as_raw();
        device.write(&sect)?;

        // Clear chainmap
        let mut fs = Self::new(device).unwrap();

        for i in 0..bootsector.block_map_count {
            fs.write_block(i as u64, 0);
        }

        // First block is always set as reserved
        fs.write_block(0, 0xFFFF_FFFF_FFFF_FFFF);

        // And finally, create a root directory.
        fs.create_root_directory()?;

        Ok(())
    }

    pub fn find_block(&mut self) -> Option<u64> {
        for i in 0..self.bootsector.block_map_count {
            let blk = self.get_block(i as _);

            if let Some(0) = blk {
                return Some(i as u64);
            }
        }

        None
    }

    pub fn get_block(&mut self, nr: u64) -> Option<u64> {
        if nr >= self.bootsector.block_map_count as u64 {
            return None;
        }

        let offset = self.bootsector.sector_size as u64 + (nr * 8);
        let mut block_raw: [u8; 8] = [0; 8];

        self.device.seek(Start(offset as _)).unwrap();
        self.device.read(&mut block_raw).unwrap();

        Some(u64::from_le_bytes(block_raw))
    }

    pub fn write_block(&mut self, nr: u64, value: u64) {
        if nr >= self.bootsector.block_map_count as u64 {
            return;
        }

        let offset = self.bootsector.sector_size as u64 + (nr * 8);
        let block_raw: [u8; 8] = value.to_le_bytes();

        self.device.seek(Start(offset as _)).unwrap();
        self.device.write(&block_raw).unwrap();
    }

    pub fn allocate_blocks(&mut self, count: u32) -> Option<u64> {
        if count == 0 {
            return None;
        }

        let first_block: Option<u64> = self.find_block();
        let mut previous_block: Option<u64> = first_block;

        for _ in 0..count {
            let new_block = self.find_block().unwrap();
            println!("Found new block: {}", new_block);

            self.write_block(previous_block.unwrap(), new_block);
            self.write_block(new_block, 0xFFFF_FFFF_FFFF_FFFF);

            previous_block = Some(new_block);
        }

        // Last block in chain
        self.write_block(previous_block.unwrap(), 0xFFFF_FFFF_FFFF_FFFF);

        first_block
    }

    pub fn get_chain(&mut self, start_block: u64) -> Box<[u64]> {
        let mut blocks: Vec<u64> = vec![];
        let mut current_block = start_block;

        while let Some(block) = self.get_block(current_block) {
            blocks.push(current_block);

            current_block = block;
        }

        blocks.into_boxed_slice()
    }

    pub fn free_blocks(&mut self, start_block: u64) {
        if start_block == 0 {
            return;
        }

        let mut current_block = start_block;

        while let Some(block) = self.get_block(current_block) {
            println!("Clear block: {}", current_block);

            if block == 0xFFFF_FFFF_FFFF_FFFF {
                self.write_block(current_block, 0);
                break;
            }

            self.write_block(current_block, 0);

            current_block = block;
        }
    }

    pub fn extend_chain_by(&mut self, start_block: u64, count: usize) {
        let chain = self.get_chain(start_block);

        let last = chain.last().unwrap();

        let allocated = self.allocate_blocks(count as u32).unwrap();

        self.write_block(*last, allocated);
    }

    pub fn shrink_chain_by(&mut self, start_block: u64, count: usize) {
        let chain = self.get_chain(start_block);

        if count == 0 {
            return;
        }

        if count > chain.len() {
            return;
        }

        let work_area = &chain[chain.len() - count - 1..];

        self.write_block(work_area[0], 0xFFFF_FFFF_FFFF_FFFF);

        for i in &work_area[1..] {
            self.write_block(*i, 0);
        }
    }

    pub fn set_chain_size(&mut self, start_block: u64, count: usize) {
        let chain_length = self.get_chain(start_block).len();

        if chain_length == count {
            return;
        } else if chain_length > count {
            self.shrink_chain_by(start_block, chain_length - count);
        } else if count > chain_length {
            self.extend_chain_by(start_block, count - chain_length);
        }
    }

    pub fn allocate_bytes(&mut self, byte_count: usize) -> Option<u64> {
        let blocks = byte_count.div_ceil(self.bootsector.block_size as usize);

        self.allocate_blocks(blocks as _)
    }

    #[inline]
    pub fn datazone_offset(&self) -> usize {
        self.bootsector.sector_size as usize
            + (self.bootsector.first_root_entity_lba as usize
                * self.bootsector.sector_size as usize)
    }

    #[inline]
    pub fn datazone_offset_with_block(&self, block: u64) -> u64 {
        self.datazone_offset() as u64 + (block as u64 * self.bootsector.block_size as u64)
    }

    pub fn read_blocks_data(
        &mut self,
        start_block: u64,
        data: &mut [u8],
        offset: u64,
    ) -> io::Result<()> {
        let chain = self.get_chain(start_block);
        let chain_off = offset / self.bootsector.block_size as u64;
        let first_occurency_offset = offset % self.bootsector.block_size as u64;

        if chain_off as usize > chain.len() {
            return Ok(());
        }

        let mut data_length = data.len();

        for (nr, &i) in chain.iter().enumerate() {
            let f_offset = self.datazone_offset_with_block(i);

            self.device.seek(Start(f_offset))?;

            let data_offset = nr as u64 * self.bootsector.block_size as u64;
            let mut read_size = if data_length < self.bootsector.block_size as usize {
                data_length
            } else {
                self.bootsector.block_size as usize
            };

            if nr == 0 {
                self.device.seek(Current(first_occurency_offset as _))?;
                
                read_size -= first_occurency_offset as usize;    
            }

            let end_offset = data_offset + read_size as u64;

            println!("{:?}", data_offset as usize..end_offset as usize);

            self.device
                .read(&mut data[data_offset as usize..end_offset as usize])?;

            data_length -= read_size;
        }

        Ok(())
    }

    pub fn write_blocks_data(
        &mut self,
        start_block: u64,
        data: &[u8],
        offset: u64,
    ) -> io::Result<()> {
        let chain = self.get_chain(start_block);
        let chain_off = offset / self.bootsector.block_size as u64;
        let first_occurency_offset = offset % self.bootsector.block_size as u64;

        if chain_off as usize > chain.len() {
            return Ok(());
        }

        let mut data_length = data.len();

        for (nr, &i) in chain.iter().enumerate() {
            let f_offset = self.datazone_offset_with_block(i);

            self.device.seek(Start(f_offset))?;

            let data_offset = nr as u64 * self.bootsector.block_size as u64;
            let mut write_size = if data_length < self.bootsector.block_size as usize {
                data_length
            } else {
                self.bootsector.block_size as usize
            };

            if nr == 0 {
                self.device.seek(Current(first_occurency_offset as _))?;

                write_size -= first_occurency_offset as usize;
            }

            let end_offset = data_offset + write_size as u64;

            println!("{:?}", data_offset as usize..end_offset as usize);

            self.device
                .write(&data[data_offset as usize..end_offset as usize])?;

            data_length -= write_size;
        }

        Ok(())
    }

    fn create_root_directory(&mut self) -> io::Result<u64> {
        let block = self.allocate_blocks(1);
        let block_container = self.allocate_blocks(1);
        let entity = entity::Entity::directory("/", 0, block_container.unwrap());
        let data = entity.as_raw();

        self.write_blocks_data(block.unwrap(), &data, 0)?;

        Ok(block.unwrap())
    }

    // pub fn create_directory(&mut self, directory_block: u64, name: String) {

    // }
}
