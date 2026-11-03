use nix::errno::Errno;
use nix::sys::signal::{Signal, kill};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::{ForkResult, Pid, execvp, fork};
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::flag;
use std::error::Error;
use std::ffi::CString;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

pub fn run_init(command: &[CString]) -> Result<i32, Box<dyn Error>> {
    let term = Arc::new(AtomicBool::new(false));
    let int = Arc::new(AtomicBool::new(false));
    let hup = Arc::new(AtomicBool::new(false));

    flag::register(SIGTERM, Arc::clone(&term))?;
    flag::register(SIGINT, Arc::clone(&int))?;
    flag::register(SIGHUP, Arc::clone(&hup))?;

    let app_pid = match unsafe { fork()? } {
        ForkResult::Parent { child } => child,
        ForkResult::Child => match execvp(&command[0], command) {
            Ok(_) => unreachable!(),
            Err(err) => {
                eprintln!("[init] execvp failed: {err}");
                std::process::exit(127);
            }
        },
    };

    println!("[init] PID 1, application PID: {app_pid}");

    loop {
        for (received, signal) in [
            (&term, Signal::SIGTERM),
            (&int, Signal::SIGINT),
            (&hup, Signal::SIGHUP),
        ] {
            if received.swap(false, Ordering::Relaxed) {
                match kill(app_pid, signal) {
                    Ok(()) | Err(Errno::ESRCH) => {}
                    Err(err) => return Err(err.into()),
                }
            }
        }

        let status = match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
            Ok(status) => status,
            Err(Errno::EINTR) => continue,
            Err(err) => return Err(err.into()),
        };

        match status {
            WaitStatus::Exited(pid, code) => {
                println!("[init] reaped PID {pid}, code {code}");
                if pid == app_pid {
                    return Ok(code);
                }
            }

            WaitStatus::Signaled(pid, signal, _) => {
                println!("[init] reaped PID {pid}, signal {signal}");
                if pid == app_pid {
                    return Ok(128 + signal as i32);
                }
            }

            WaitStatus::StillAlive => {
                thread::sleep(Duration::from_millis(50));
            }

            _ => {}
        }
    }
}
