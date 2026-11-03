use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::sched::{CloneFlags, unshare};
use nix::sys::stat::{Mode, SFlag, makedev, mknod};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, execvp, fork, pivot_root, sethostname};
use std::fs;
use std::os::unix::fs::PermissionsExt;

use std::env;
use std::error::Error;
use std::ffi::CString;

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

fn setup_dev() -> Result<(), Box<dyn Error>> {
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

fn setup_tmp() -> Result<(), Box<dyn Error>> {
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

    unshare(
        CloneFlags::CLONE_NEWNS
            | CloneFlags::CLONE_NEWUTS
            | CloneFlags::CLONE_NEWIPC
            | CloneFlags::CLONE_NEWPID,
    )?; // unshare(2) syscall. mount namespace, nis namespace, ipc namespace, pid namespace

    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_PRIVATE | MsFlags::MS_REC,
        None::<&str>,
    )?; // make mount propagation private

    sethostname("mini-runc")?;
    match unsafe { fork()? } {
        /*
         * fork() syscall creates copy of itselfs as child-process.
         * parent process waits exit or signaled of child process.
         * */
        ForkResult::Parent { child } => {
            println!("[parent] Child PID: {child}");

            match waitpid(child, None)? {
                WaitStatus::Exited(_, code) => std::process::exit(code),
                WaitStatus::Signaled(_, signal, _) => {
                    std::process::exit(128 + signal as i32);
                }
                status => {
                    eprintln!("Unexcepted status: {status:?}");
                }
            }
        }

        ForkResult::Child => {
            let rootfs = std::fs::canonicalize("rootfs")?;
            let old_root = rootfs.join(".old_root");

            mount(
                Some(rootfs.as_path()),
                rootfs.as_path(),
                None::<&str>,
                MsFlags::MS_BIND | MsFlags::MS_REC,
                None::<&str>,
            )?;

            pivot_root(&rootfs, &old_root)?;
            std::env::set_current_dir("/")?;

            mount(
                Some("proc"),
                "/proc",
                Some("proc"),
                MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC,
                None::<&str>,
            )?;

            setup_dev()?;
            setup_tmp()?;

            execvp(&command[0], &command)?;
        }
    }
    Ok(())
}
