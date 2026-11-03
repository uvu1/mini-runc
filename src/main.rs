use nix::mount::{MsFlags, mount};
use nix::sched::{CloneFlags, unshare};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, execvp, fork, sethostname};

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

    unshare(
        CloneFlags::CLONE_NEWNS
            | CloneFlags::CLONE_NEWUTS
            | CloneFlags::CLONE_NEWIPC
            | CloneFlags::CLONE_NEWPID,
    )?;

    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_PRIVATE | MsFlags::MS_REC,
        None::<&str>,
    )?;

    sethostname("mini-runc")?;
    match unsafe { fork()? } {
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
            mount(
                Some("proc"),
                "/proc",
                Some("proc"),
                MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC,
                None::<&str>,
            )?;

            execvp(&command[0], &command)?;
        }
    }

    Ok(())
}
