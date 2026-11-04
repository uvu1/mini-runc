use futures_util::TryStreamExt;
use nix::{
    sched::{CloneFlags, unshare},
    unistd::Pid,
};
use rtnetlink::{LinkUnspec, LinkVeth, RouteMessageBuilder, new_connection};
use std::{error::Error, net::Ipv4Addr, process::Command};
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

        let host = handle
            .link()
            .get()
            .match_name("mr-host0")
            .execute()
            .try_next()
            .await?
            .ok_or_else(|| io::Error::other("mr-host0 not found"))?;
        let host_index = host.header.index;

        for args in [
            vec!["-d", "link", "show", "dev", "eth0"],
            vec!["-4", "addr", "show", "dev", "eth0"],
            vec!["-4", "route", "show"],
            vec!["-4", "route", "get", "10.200.0.1"],
        ] {
            let output = Command::new("ip").args(&args).output()?;
            eprintln!(
                "$ ip {}\n{}{}",
                args.join(" "),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            )
        }

        handle
            .address()
            .add(host_index, Ipv4Addr::new(10, 200, 0, 1).into(), 24)
            .execute()
            .await?;

        handle
            .link()
            .change(LinkUnspec::new_with_index(host_index).up().build())
            .execute()
            .await?;
        Ok(())
    })
}

pub fn cleanup_host() -> Result<(), Box<dyn Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()?;

    rt.block_on(async {
        let (connection, handle, _) = new_connection()?;
        tokio::spawn(connection);

        let link = handle
            .link()
            .get()
            .match_name("mr-host0")
            .execute()
            .try_next()
            .await?;

        if let Some(link) = link {
            handle.link().del(link.header.index).execute().await?;
        }

        Ok(())
    })
}

pub fn setup_container() -> Result<(), Box<dyn Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()?;

    rt.block_on(async {
        let (connection, handle, _) = new_connection()?;
        tokio::spawn(connection);

        let lo = handle
            .link()
            .get()
            .match_name("lo")
            .execute()
            .try_next()
            .await?
            .ok_or_else(|| io::Error::other("lo not found"))?;

        let peer = handle
            .link()
            .get()
            .match_name("mr-peer0")
            .execute()
            .try_next()
            .await?
            .ok_or_else(|| io::Error::other("mr-peer0 not found"))?;

        let peer_index = peer.header.index;

        handle
            .link()
            .change(LinkUnspec::new_with_index(peer_index).name("eth0").build())
            .execute()
            .await?;

        handle
            .link()
            .change(LinkUnspec::new_with_index(peer_index).up().build())
            .execute()
            .await?;

        handle
            .link()
            .change(LinkUnspec::new_with_index(lo.header.index).up().build())
            .execute()
            .await?;

        let route = RouteMessageBuilder::<Ipv4Addr>::new()
            .gateway(Ipv4Addr::new(10, 200, 0, 1))
            .output_interface(peer_index)
            .build();

        handle.route().add(route).execute().await?;

        Ok(())
    })
}
