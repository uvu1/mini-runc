mod container;
mod init;
mod network;
mod rootfs;

use std::env;
use std::error::Error;
use std::ffi::CString;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);

    if args.next().as_deref() != Some("run") {
        eprintln!("Usage: minirun run <command> [args...]");
        std::process::exit(2);
    }

    let command: Vec<CString> = args.map(CString::new).collect::<Result<_, _>>()?;

    if command.is_empty() {
        return Err("command is required".into());
    }
    container::create_container(&command)?;

    Ok(())
}
