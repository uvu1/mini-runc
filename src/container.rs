use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::sched::{CloneFlags, unshare};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, fork, pivot_root, sethostname};

use std::error::Error;
use std::ffi::CString;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use crate::init::run_init;
use crate::network;
use crate::rootfs::{setup_dev, setup_tmp};

pub fn create_container(command: &[CString]) -> Result<(), Box<dyn Error>> {
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

    let (mut parent_sock, mut child_sock) = UnixStream::pair()?;

    match unsafe { fork()? } {
        /*
         * fork() syscall creates copy of itselfs as child-process.
         * parent process waits exit or signaled of child process.
         * */
        ForkResult::Parent { child } => {
            drop(child_sock);
            println!("[parent] Child PID: {child}");

            let setup_result = || -> Result<(), Box<dyn Error>> {
                let mut ready = [0u8; 1];
                parent_sock.read_exact(&mut ready)?;

                if ready != [b'R'] {
                    return Err("Invalid READY Message".into());
                }

                network::setup_host(child)?;

                parent_sock.write_all(b"G")?;

                Ok(())
            }();

            if let Err(err) = setup_result {
                drop(parent_sock);
                let _ = network::cleanup_host();
                let _ = waitpid(child, None);
                return Err(err);
            }

            drop(parent_sock);

            let status = waitpid(child, None);
            network::cleanup_host()?;

            match status? {
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
            drop(parent_sock);
            network::isolate()?;

            child_sock.write_all(b"R")?;

            let mut response = [0u8; 1];
            child_sock.read_exact(&mut response)?;

            if response != [b'G'] {
                return Err("Invalid CONTINUE Message".into());
            }

            drop(child_sock);

            network::setup_container()?;

            let rootfs = std::fs::canonicalize("rootfs")?;
            let old_root = rootfs.join(".old_root");
            fs::create_dir_all(&old_root)?;

            mount(
                Some(rootfs.as_path()),
                rootfs.as_path(),
                None::<&str>,
                MsFlags::MS_BIND | MsFlags::MS_REC,
                None::<&str>,
            )?;

            pivot_root(&rootfs, &old_root)?;
            std::env::set_current_dir("/")?;
            umount2("/.old_root", MntFlags::MNT_DETACH)?;
            fs::remove_dir("/.old_root")?;

            mount(
                Some("proc"),
                "/proc",
                Some("proc"),
                MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC,
                None::<&str>,
            )?;

            setup_dev()?;
            setup_tmp()?;

            let exit_code = run_init(command)?;
            std::process::exit(exit_code);
        }
    }
    Ok(())
}
