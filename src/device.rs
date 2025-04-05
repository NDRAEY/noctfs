use no_std_io::io::{Read, Seek, Write};

pub trait Device: Read + Seek + Write {}