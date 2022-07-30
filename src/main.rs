use swayipc::{Connection, Error, Event, EventType, Fallible, Node, NodeType, WindowChange};

fn walk_tree(root: Node, apply: &dyn Fn(&Node)) {
    apply(&root);
    root.nodes.into_iter().for_each(|n| walk_tree(n, apply));
}

fn setpriority(pid: i32, prio: i32) -> Fallible<()> {
    if unsafe { libc::setpriority(libc::PRIO_PROCESS, pid.try_into().unwrap(), prio) } != 0 {
        Err(Error::CommandFailed(format!(
            "failed to set priority for pid {} to {}",
            pid, prio
        )))
    } else {
        Ok(())
    }
}

fn main() -> Fallible<()> {
    for event in Connection::new()?.subscribe([EventType::Window])? {
        match event? {
            Event::Window(w) => match w.change {
                WindowChange::Focus => {
                    let mut conn = Connection::new()?;
                    walk_tree(conn.get_tree()?, &|n| match n.node_type {
                        NodeType::Con | NodeType::FloatingCon => {
                            if let Some(true) = n.visible {
                                setpriority(n.pid.unwrap(), 0).unwrap();
                            } else {
                                setpriority(n.pid.unwrap(), 10).unwrap();
                            }
                        }
                        _ => (),
                    });
                }
                _ => (),
            },
            _ => unreachable!(),
        }
    }
    Ok(())
}
