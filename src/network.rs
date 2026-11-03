use futures_util::TryStreamExt;
use nix::{
    sched::{CloneFlags, unshare},
    unistd::Pid,
};
use rtnetlink::{LinkUnspec, LinkVeth, new_connection};
use std::error::Error;
use tokio::io;

pub fn isolate() -> Result<(), Box<dyn Error>> {
    unshare(CloneFlags::CLONE_NEWNET)?;
    Ok(())
}

pub fn setup_host(child: Pid) -> Result<(), Box<dyn Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()?;

    rt.block_on(async {
        let (connection, handle, _) = new_connection()?;
        tokio::spawn(connection);

        handle
            .link()
            .add(LinkVeth::new("mr-host0", "mr-peer0").build())
            .execute()
            .await?;

        let peer = handle
            .link()
            .get()
            .match_name("mr-peer0")
            .execute()
            .try_next()
            .await?
            .ok_or_else(|| io::Error::other("mr-peer0 not found"))?;

        handle
            .link()
            .change(
                LinkUnspec::new_with_index(peer.header.index)
                    .setns_by_pid(child.as_raw() as u32)
                    .build(),
            )
            .execute()
            .await?;

        Ok(())
    })
}
