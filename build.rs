use std::process::Command;

fn main() {
    let mut command = Command::new("nasm");
    let prog = command.arg("-fbin")
        .arg("static/bootcode.asm")
        .arg("-o")
        .arg("static/bootcode.bin");

    let result = prog.spawn();
    result.unwrap().wait().unwrap();
}
