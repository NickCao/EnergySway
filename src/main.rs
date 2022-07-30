#![feature(async_closure)]
use async_recursion::async_recursion;
use std::future::Future;
use swayipc::{Connection, Event, EventType, Node, NodeType, WindowChange};
use zbus::zvariant;
use zbus_systemd::systemd1::UnitProxy;

#[async_recursion]
async fn walk_tree<'a, 'b: 'a, T>(
    root: &'a Node,
    apply: impl Fn(&'a Node) -> T
        + std::marker::Copy
        + std::marker::Sync
        + std::marker::Send
        + 'async_recursion,
) where
    T: Future<Output = ()> + 'b + std::marker::Send + 'async_recursion,
{
    apply(&root).await;
    for node in root.nodes.iter() {
        walk_tree(&node, apply).await;
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    for event in Connection::new()?.subscribe([EventType::Window])? {
        match event? {
            Event::Window(w) => match w.change {
                WindowChange::Focus => {
                    let mut conn = Connection::new()?;
                    walk_tree(&conn.get_tree()?, async move |node| {
                        let conn = zbus::Connection::session().await.unwrap();
                        let manager = zbus_systemd::systemd1::ManagerProxy::new(&conn)
                            .await
                            .unwrap();
                        match node.node_type {
                            NodeType::Con | NodeType::FloatingCon => {
                                if let Ok(unit) = manager
                                    .get_unit_by_pid(node.pid.unwrap().try_into().unwrap())
                                    .await
                                {
                                    let unit = UnitProxy::builder(&conn)
                                        .path(unit)
                                        .unwrap()
                                        .build()
                                        .await
                                        .unwrap();
                                    let quota = if node.visible.unwrap() {
                                        u64::MAX
                                    } else {
                                        1e5 as u64
                                    };
                                    unit.set_properties(
                                        true,
                                        vec![(
                                            "CPUQuotaPerSecUSec".to_string(),
                                            zvariant::Value::U64(quota).into(),
                                        )],
                                    )
                                    .await
                                    .unwrap();
                                }
                            }
                            _ => (),
                        }
                    })
                    .await;
                }
                _ => (),
            },
            _ => unreachable!(),
        }
    }
    Ok(())
}
