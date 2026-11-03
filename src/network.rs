use std::error::Error;

use nix::sched::{CloneFlags, unshare};
pub fn isolate() -> Result<(), Box<dyn Error>> {
    unshare(CloneFlags::CLONE_NEWNET)?;
    Ok(())
}
