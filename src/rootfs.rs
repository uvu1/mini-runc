use nix::mount::{MsFlags, mount};
use nix::sys::stat::{Mode, SFlag, makedev, mknod};
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn create_char_device(path: &str, major: u64, minor: u64) -> Result<(), Box<dyn Error>> {
    mknod(
        path,
        SFlag::S_IFCHR,
        Mode::from_bits_truncate(0o666),
        makedev(major, minor),
    )?;

    fs::set_permissions(path, fs::Permissions::from_mode(0o666))?;

    Ok(())
}

pub fn setup_dev() -> Result<(), Box<dyn Error>> {
    fs::create_dir_all("/dev")?;

    mount(
        Some("tmpfs"),
        "/dev",
        Some("tmpfs"),
        MsFlags::MS_NOSUID | MsFlags::MS_NOEXEC,
        Some("mode=0755,size=16m"),
    )?;

    create_char_device("/dev/null", 1, 3)?;
    create_char_device("/dev/zero", 1, 5)?;
    create_char_device("/dev/random", 1, 8)?;
    create_char_device("/dev/urandom", 1, 9)?;
    create_char_device("/dev/tty", 5, 0)?;

    Ok(())
}

pub fn setup_tmp() -> Result<(), Box<dyn Error>> {
    fs::create_dir_all("/tmp")?;

    mount(
        Some("tmpfs"),
        "/tmp",
        Some("tmpfs"),
        MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC,
        Some("mode=1777,size=64m"),
    )?;

    Ok(())
}
