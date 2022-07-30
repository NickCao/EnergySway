#![feature(async_closure)]
use async_recursion::async_recursion;
use std::future::Future;
use swayipc::{Connection, Event, EventType, Node, NodeType, WindowChange};
use zbus::zvariant;
use zbus_systemd::systemd1::{ManagerProxy, ScopeProxy, UnitProxy};

#[async_recursion]
async fn walk_tree<'a, 'b: 'a, T>(
    root: &'a Node,
    apply: impl Fn(&'a Node) -> T
        + std::marker::Copy
        + std::marker::Sync
        + std::marker::Send
        + 'async_recursion,
) -> anyhow::Result<()>
where
    T: Future<Output = anyhow::Result<()>> + 'b + std::marker::Send + 'async_recursion,
{
    apply(&root).await?;
    for node in root.nodes.iter() {
        walk_tree(&node, apply).await?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    simple_logger::init_with_level(log::Level::Info)?;
    for event in Connection::new()?.subscribe([EventType::Window])? {
        match event? {
            Event::Window(w) => match w.change {
                WindowChange::New
                | WindowChange::Move
                | WindowChange::Close
                | WindowChange::FullscreenMode
                | WindowChange::Floating
                | WindowChange::Focus => {
                    log::info!("received event {:?}", w.change);
                    walk_tree(
                        &Connection::new()?.get_tree()?,
                        async move |node| -> anyhow::Result<()> {
                            let conn = zbus::Connection::session().await?;
                            let manager = ManagerProxy::new(&conn).await?;
                            match node.node_type {
                                NodeType::Con | NodeType::FloatingCon => {
                                    let pid: u32 = node.pid.unwrap().try_into()?;
                                    if let Ok(unit) = manager.get_unit_by_pid(pid).await {
                                        let scope = ScopeProxy::builder(&conn)
                                            .path(unit.clone())?
                                            .build()
                                            .await?;
                                        let unit =
                                            UnitProxy::builder(&conn).path(unit)?.build().await?;
                                        let quota = if node.visible.unwrap() {
                                            u64::MAX
                                        } else {
                                            1e5 as u64
                                        };
                                        let current_quota: u64 =
                                            scope.cpu_quota_per_sec_u_sec().await?;
                                        if current_quota != quota {
                                            log::info!(
                                                "setting CPUQuotaPerSecUSec of {} from {} to {}",
                                                unit.id().await?,
                                                current_quota,
                                                quota
                                            );
                                            unit.set_properties(
                                                true,
                                                vec![(
                                                    "CPUQuotaPerSecUSec".to_string(),
                                                    zvariant::Value::U64(quota).into(),
                                                )],
                                            )
                                            .await?;
                                        }
                                    } else {
                                        log::info!("no matching unit found for pid {}", pid);
                                    }
                                    Ok(())
                                }
                                _ => {
                                    log::debug!(
                                        "ignored node {:?}:{:?}",
                                        node.node_type,
                                        node.name
                                    );
                                    Ok(())
                                }
                            }
                        },
                    )
                    .await?;
                }
                _ => log::debug!("ignored event {:?}", w.change),
            },
            _ => unreachable!(),
        }
    }
    Ok(())
}
