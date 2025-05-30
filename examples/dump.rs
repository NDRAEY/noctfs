use std::fs::OpenOptions;
use std::{
    fs::File,
    io::{Read, Seek, Write},
};

use no_std_io::io::{self, Error as NoStdError, ErrorKind};
use noctfs::{device::Device, NoctFS};

struct FileDevice(File);

impl io::Read for FileDevice {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf).map_err(|_err| {
            eprintln!("{}", _err);
            NoStdError::new(ErrorKind::Other, "unknown")
        })
    }
}

impl io::Write for FileDevice {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf).map_err(|_err| {
            eprintln!("{}", _err);
            NoStdError::new(ErrorKind::Other, "unknown")
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush().map_err(|_err| {
            eprintln!("{}", _err);
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

    let mut fs = NoctFS::new(&mut device).unwrap();

    let start_block = fs.get_root_entity().unwrap().start_block;
    let list = fs.list_directory(start_block);

    for i in list {
        println!("{:?}", i);
    }

    Ok(())
}
