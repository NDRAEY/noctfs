use std::fs::OpenOptions;
use std::io::Result;
use std::{
    fs::File,
    io::{Error, Read, Seek, Write},
};

use no_std_io::io::{self, Error as NoStdError, ErrorKind};
use noctfs::entity::{Entity, EntityFlags};
use noctfs::{device::Device, NoctFS};

struct FileDevice(File);

impl io::Read for FileDevice {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf).map_err(|_err| {
            eprintln!("{}", _err.to_string());
            NoStdError::new(ErrorKind::Other, "unknown")
        })
    }
}

impl io::Write for FileDevice {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf).map_err(|_err| {
            eprintln!("{}", _err.to_string());
            NoStdError::new(ErrorKind::Other, "unknown")
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush().map_err(|_err| {
            eprintln!("{}", _err.to_string());
            NoStdError::new(ErrorKind::Other, "unknown")
        })
    }
}

impl io::Seek for FileDevice {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.0
            .seek({
                match pos {
                    io::SeekFrom::Start(a) => std::io::SeekFrom::Start(a),
                    io::SeekFrom::End(a) => std::io::SeekFrom::End(a),
                    io::SeekFrom::Current(a) => std::io::SeekFrom::Current(a),
                }
            })
            .map_err(|_| NoStdError::new(ErrorKind::Other, "unknown"))
    }
}

impl Device for FileDevice {}

fn main() -> std::io::Result<()> {
    let filename = std::env::args().skip(1).last().expect("No filename!");

    let file = OpenOptions::new().read(true).write(true).open(filename)?;
    let mut device = FileDevice(file);

    NoctFS::format(&mut device, None, Some(512))
        .map_err(|a| Error::new(std::io::ErrorKind::Other, a.to_string()))?;

    let fs = NoctFS::new(&mut device);

    if let Err(ref e) = fs {
        println!("Error opening filesystem: {:?}", e);
    }

    let mut fs = fs.unwrap();
    let re = fs.get_root_entity().unwrap();

    let system_folder = fs.create_directory(re.start_block, "System");
    let users_folder = fs.create_directory(re.start_block, "Users");
    let apps_folder = fs.create_directory(re.start_block, "Applications");

    let config_folder = fs.create_directory(system_folder.start_block, "Config");

    fs.create_directory(users_folder.start_block, "NDRAEY");
    fs.create_directory(users_folder.start_block, "User1");
    fs.create_directory(users_folder.start_block, "User2");
    fs.create_directory(users_folder.start_block, "User3");
    fs.create_directory(users_folder.start_block, "Your mum");

    fs.create_directory(apps_folder.start_block, "Binaries");
    fs.create_directory(apps_folder.start_block, "Shared Libraries");
    fs.create_directory(apps_folder.start_block, "Audacity");
    fs.create_directory(apps_folder.start_block, "GIMP");
    fs.create_directory(apps_folder.start_block, "Holop Rukozhop");
    fs.create_directory(apps_folder.start_block, "Deva IDE");
    fs.create_directory(apps_folder.start_block, "Visual Studio Code");
    fs.create_directory(apps_folder.start_block, "Mozilla Firefox");
    fs.create_directory(apps_folder.start_block, "Google Chrome");
    fs.create_directory(apps_folder.start_block, "Blender");
    fs.create_directory(apps_folder.start_block, "Web Applications");
    fs.create_directory(apps_folder.start_block, "Ristretto");
    fs.create_directory(apps_folder.start_block, "Pavi");
    fs.create_directory(apps_folder.start_block, "Wireshark");
    fs.create_directory(apps_folder.start_block, "Videolan VLC");
    fs.create_directory(apps_folder.start_block, "Calibre");
    fs.create_directory(apps_folder.start_block, "Qt");

    fs.create_file(config_folder.start_block, "system_info.cfg");
    let pkg_r = fs.create_file(config_folder.start_block, "pkg.cfg");

    fs.delete_file(config_folder.start_block, &pkg_r);
    
    fn list_dir(fs: &mut NoctFS<'_>, dir: &Entity, level: usize) {
        let ents = fs.list_directory(dir.start_block);
        
        for i in ents {
            let mut name = i.name.clone(); 
            
            if i.flags.contains(EntityFlags::DIRECTORY) {
                name += "/";
            }

            println!("{} - {} (blk: {}, size: {})", " ".repeat(level * 4), name, i.start_block, i.size);

            if i.flags.contains(EntityFlags::DIRECTORY) {
                list_dir(fs, &i, level + 1);
            }
        }
    }

    list_dir(&mut fs, &re, 0);
    
    Ok(())
}
