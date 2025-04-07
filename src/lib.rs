// #![no_std]

extern crate alloc;

use alloc::vec;
use alloc::{boxed::Box, vec::Vec};
use arrayref::array_ref;
use bootsector::BootSector;
use device::Device;
use entity::{Entity, EntityFlags};
use no_std_io::io::{
    self, Error,
    SeekFrom::{Current, End, Start},
};

pub mod bootsector;
pub mod device;
pub mod entity;

pub type BlockAddress = u64;

const ALLOWED_BLOCK_SIZES: &[u32] = &[512, 1024, 2048, 4096, 8192, 16384];
const DEFAULT_BLOCK_SIZE: &u32 = &ALLOWED_BLOCK_SIZES[4]; // 8192
const DEFAULT_SECTOR_SIZE: usize = 512;
const FILESYSTEM_CODENAME: &[u8] = b"NoctFS__";

const BLOCK_ADDRESS_SIZE: usize = core::mem::size_of::<BlockAddress>();

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

    pub fn format(
        device: &'dev mut dyn Device,
        sector_size: Option<usize>,
        block_size: Option<usize>,
    ) -> io::Result<()> {
        let size = device.seek(End(0))?;
        device.seek(Start(0))?;

        let mut bootsector = BootSector::with_data(
            size.try_into().unwrap(),
            sector_size.unwrap_or(DEFAULT_SECTOR_SIZE) as _,
            block_size.unwrap_or(*DEFAULT_BLOCK_SIZE as usize) as _,
        );

        bootsector.first_root_entity_block = 1;

        // Write bootsector

        let sect = bootsector.as_raw();
        device.write(&sect)?;

        // Clear chainmap
        let mut fs = Self::new(device).unwrap();

        // Overwrite first 1MB
        let empty_block = vec![0u8; 1 << 20];
        fs.device.seek(Start(fs.datazone_offset_with_block(1)))?;
        fs.device.write(&empty_block)?;

        for i in 0..bootsector.block_map_count {
            fs.write_block(i as BlockAddress, 0);
        }

        // First block is always set as reserved
        fs.write_block(0, 0xFFFF_FFFF_FFFF_FFFF);

        // And finally, create a root directory.
        fs.create_root_directory()?;

        Ok(())
    }

    pub fn block_size(&self) -> usize {
        self.bootsector.block_size as usize
    }

    pub fn find_block(&mut self) -> Option<BlockAddress> {
        for i in 0..self.bootsector.block_map_count {
            let blk = self.get_block(i as _);

            if let Some(0) = blk {
                return Some(i as u64);
            }
        }

        None
    }

    pub fn get_block(&mut self, nr: BlockAddress) -> Option<BlockAddress> {
        if nr >= self.bootsector.block_map_count as u64 {
            return None;
        }

        let offset =
            self.bootsector.sector_size as BlockAddress + (nr * BLOCK_ADDRESS_SIZE as BlockAddress);
        let mut block_raw: [u8; BLOCK_ADDRESS_SIZE] = [0; BLOCK_ADDRESS_SIZE];

        self.device.seek(Start(offset as _)).unwrap();
        self.device.read(&mut block_raw).unwrap();

        Some(u64::from_le_bytes(block_raw))
    }

    pub fn write_block(&mut self, nr: BlockAddress, value: BlockAddress) {
        if nr >= self.bootsector.block_map_count as BlockAddress {
            return;
        }

        let offset =
            self.bootsector.sector_size as BlockAddress + (nr * BLOCK_ADDRESS_SIZE as BlockAddress);
        let block_raw: [u8; BLOCK_ADDRESS_SIZE] = value.to_le_bytes();

        self.device.seek(Start(offset as _)).unwrap();
        self.device.write(&block_raw).unwrap();
    }

    pub fn allocate_blocks(&mut self, count: u32) -> Option<BlockAddress> {
        if count == 0 {
            return None;
        }

        let first_block = self.find_block();
        let mut previous_block = first_block;

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

    pub fn get_chain(&mut self, start_block: BlockAddress) -> Box<[u64]> {
        let mut blocks: Vec<BlockAddress> = vec![];
        let mut current_block = start_block;

        while let Some(block) = self.get_block(current_block) {
            blocks.push(current_block);

            current_block = block;
        }

        blocks.into_boxed_slice()
    }

    pub fn free_blocks(&mut self, start_block: BlockAddress) {
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

    pub fn extend_chain_by(&mut self, start_block: BlockAddress, count: usize) {
        let chain = self.get_chain(start_block);

        let last = chain.last().unwrap();

        let allocated = self.allocate_blocks(count as u32).unwrap();

        self.write_block(*last, allocated);
    }

    pub fn shrink_chain_by(&mut self, start_block: BlockAddress, count: usize) {
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

    pub fn set_chain_size(&mut self, start_block: BlockAddress, count: usize) {
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
            + (BLOCK_ADDRESS_SIZE * self.bootsector.block_map_count as usize)
    }

    #[inline]
    pub fn datazone_offset_with_block(&self, block: BlockAddress) -> BlockAddress {
        self.datazone_offset() as BlockAddress
            + (block as BlockAddress * self.bootsector.block_size as BlockAddress)
    }

    pub fn read_blocks_data(
        &mut self,
        start_block: BlockAddress,
        data: &mut [u8],
        offset: u64,
    ) -> io::Result<()> {
        let chain = self.get_chain(start_block);
        let chain_off = (offset / self.bootsector.block_size as u64) as usize;
        let first_occurency_offset = offset % self.bootsector.block_size as u64;

        if chain_off as usize > chain.len() {
            return Ok(());
        }

        let chain = &chain[chain_off..];

        let mut data_length = data.len();

        for (nr, &i) in chain.iter().enumerate() {
            let data_offset = nr as u64 * self.bootsector.block_size as u64;
            let mut read_size = if data_length < self.bootsector.block_size as usize {
                data_length
            } else {
                self.bootsector.block_size as usize
            };
            
            if read_size == 0 {
                break;
            }
            
            let f_offset = self.datazone_offset_with_block(i);

            self.device.seek(Start(f_offset))?;

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
        start_block: BlockAddress,
        data: &[u8],
        offset: u64,
    ) -> io::Result<()> {
        let chain = self.get_chain(start_block);
        let chain_off = (offset / self.bootsector.block_size as u64) as usize;
        let first_occurency_offset = offset % self.bootsector.block_size as u64;

        if chain_off > chain.len() {
            return Ok(());
        }

        let chain = &chain[chain_off..];

        let mut data_length = data.len();

        for (nr, &i) in chain.iter().enumerate() {
            let f_offset = self.datazone_offset_with_block(i);

            self.device.seek(Start(f_offset))?;

            let data_offset = nr as u64 * self.bootsector.block_size as u64;
            let write_size = if data_length < self.bootsector.block_size as usize {
                data_length
            } else {
                self.bootsector.block_size as usize
            };

            if nr == 0 {
                self.device.seek(Current(first_occurency_offset as _))?;

                // write_size -= first_occurency_offset as usize;
            }

            let end_offset = data_offset + write_size as u64;

            // println!("{:?}", data_offset as usize..end_offset as usize);

            self.device
                .write(&data[data_offset as usize..end_offset as usize])?;

            data_length -= write_size;
        }

        Ok(())
    }

    fn create_root_directory(&mut self) -> io::Result<u64> {
        let block = self.allocate_blocks(1).unwrap();
        //let block_container = self.allocate_blocks(1);
        //let entity = Entity::directory("(root)", 0, block_container.unwrap());
        //let data = entity.as_raw();

        //self.write_blocks_data(block.unwrap(), &data, 0)?;

        let this_entity = Entity::directory(".", 0, block);
        let parent_entity = Entity::directory("..", 0, block);

        self.write_entity(block, &this_entity);
        self.write_entity(block, &parent_entity);

        Ok(block)
    }

    pub fn get_root_entity(&mut self) -> io::Result<Entity> {
        // let chain_size = self.get_chain(1).len();
        // let mut data = vec![0u8; chain_size * self.bootsector.block_size as usize];

        // self.read_blocks_data(1, data.as_mut_slice(), 0)?;

        Ok(Entity {
            name: "/".to_string(),
            size: 0,
            start_block: 1,
            flags: EntityFlags::DIRECTORY,
            vendor_data_size: 0,
        })
    }

    fn read_chain_data_vec(&mut self, start_block: BlockAddress) -> Vec<u8> {
        let chain_size = self.get_chain(start_block).len();
        let mut data = vec![0u8; chain_size * self.bootsector.block_size as usize];

        self.read_blocks_data(start_block, data.as_mut_slice(), 0)
            .unwrap();

        data
    }

    pub fn allocate_for_entity(
        &mut self,
        directory_block: BlockAddress,
        entity: &Entity,
    ) -> Option<usize> {
        let mut data = self.read_chain_data_vec(directory_block);

        let mut index = 0usize;

        // Find free space
        while index < data.len() {
            let header_size = u32::from_le_bytes(*array_ref![data[index..], 0, 4]);

            println!("[{index} / {}] Header size: {}", data.len(), header_size);

            if header_size == 0 {
                return Some(index);
            }

            index += header_size as usize + 4;

            if entity.fact_size() >= (data.len() - index) as _ {
                self.extend_chain_by(directory_block, 1);

                // println!("=== Extending chain!");

                data = self.read_chain_data_vec(directory_block);
            }
        }

        None
    }

    pub fn write_entity(&mut self, directory_block: BlockAddress, entity: &Entity) {
        let allocated = self.allocate_for_entity(directory_block, entity).unwrap();
        let mut data = self.read_chain_data_vec(directory_block);
        let raw_entity = entity.as_raw();

        data[allocated..allocated + raw_entity.len()].copy_from_slice(&raw_entity);

        self.write_blocks_data(directory_block, &data, 0).unwrap();
    }

    pub fn create_directory<T: ToString>(&mut self, directory_block: u64, name: T) -> Entity {
        let block = self.allocate_blocks(1).unwrap();
        let entity = Entity::directory(name, 0, block);

        self.write_entity(directory_block, &entity);


        let this_entity = Entity::directory(".", 0, block);
        let parent_entity = Entity::directory("..", 0, directory_block);

        self.write_entity(block, &this_entity);
        self.write_entity(block, &parent_entity);

        entity
    }

    pub fn create_file<T: ToString>(&mut self, directory_block: u64, name: T) -> Entity {
        let block = self.allocate_blocks(1).unwrap();
        let entity = Entity::file(name, 0, block);

        self.write_entity(directory_block, &entity);

        entity
    }

    pub fn get_entity_offset(
        &mut self,
        directory_block: BlockAddress,
        entity: &Entity,
    ) -> Option<usize> {
        let data = self.read_chain_data_vec(directory_block);
        let raw_data = entity.as_raw();

        let mut index = 0usize;

        while index < data.len() {
            let header_size = u32::from_le_bytes(*array_ref![data[index..], 0, 4]);

            println!("[{} / {}] Header size: {header_size}", index, data.len());

            if header_size == 0 {
                println!("empty header");
                break;
            }

            if data[index..index + raw_data.len()] == *raw_data {
                return Some(index);
            }

            index += header_size as usize + 4;
        }

        None
    }

    pub fn get_entity_by_parent_and_block(
        &mut self,
        directory_block: BlockAddress,
        entity_block: BlockAddress,
    ) -> Option<Entity> {
        let data = self.read_chain_data_vec(directory_block);
        let mut index = 0usize;

        while index < data.len() {
            let header_size = u32::from_le_bytes(*array_ref![data[index..], 0, 4]);

            if header_size == 0 {
                break;
            }

            let cur_entity = Entity::from_raw(&data[index..index + header_size as usize + 4]);
            // println!("{} {}", cur_entity.name, entity.name);

            if cur_entity.start_block == entity_block {
                return Some(cur_entity);
            }

            index += header_size as usize + 4;
        }

        None
    }

    pub fn write_contents_by_entity(
        &mut self,
        directory_block: BlockAddress,
        entity: &Entity,
        data: &[u8],
        offset: u64,
    ) {
        let block = entity.start_block;
        let data_len = data.len();

        // let chain = self.get_chain(block);

        let offset_end = data_len as u64 + offset;

        let target_chain_len = offset_end.div_ceil(self.bootsector.block_size as _) as usize;

        self.set_chain_size(block, target_chain_len);

        self.write_blocks_data(block, data, offset).unwrap();

        // Update file metadata

        let ent_offset = self.get_entity_offset(directory_block, entity).unwrap();
        let mut new_entity = entity.clone();

        new_entity.size = offset_end;

        self.write_blocks_data(directory_block, &new_entity.as_raw(), ent_offset as _)
            .unwrap();
    }

    pub fn overwrite_entity_header(&mut self, directory_block: BlockAddress, entity: &Entity, new_entity: &Entity) -> Option<()> {
        let ent_offset = self.get_entity_offset(directory_block, entity)?;

        self.write_blocks_data(directory_block, &new_entity.as_raw(), ent_offset as _).unwrap();

        Some(())
    }

    pub fn read_contents_by_entity(
        &mut self,
        entity: &Entity,
        data: &mut [u8],
        offset: u64,
    ) -> io::Result<()> {
        self.read_blocks_data(entity.start_block, data, offset)
    }

    pub fn list_directory(&mut self, directory_block: BlockAddress) -> Vec<Entity> {
        let mut ents: Vec<Entity> = vec![];

        let data = self.read_chain_data_vec(directory_block);
        let mut index = 0usize;

        while index < data.len() {
            let header_size = u32::from_le_bytes(*array_ref![data[index..index + 4], 0, 4]);

            // println!("{header_size}");

            if header_size == 0 {
                break;
            }

            let entity = Entity::from_raw(&data[index..]);

            ents.push(entity);

            index += header_size as usize + 4;
        }

        ents
    }

    pub fn delete_file(&mut self, directory_block: BlockAddress, entity: &Entity) {
        if entity.is_directory() {
            return;
        }

        let mut data = self.read_chain_data_vec(directory_block);
        let off = self.get_entity_offset(directory_block, entity).unwrap();
        let entity_size = entity.fact_size() as usize;
        let off_end = off + entity_size;

        data.copy_within(off_end.., off);

        self.free_blocks(entity.start_block);

        self.write_blocks_data(directory_block, data.as_slice(), 0)
            .unwrap();
    }
}
