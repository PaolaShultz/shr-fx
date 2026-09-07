use fx::{
    audio::{PortGraph, connections, edit_connections},
    storage::Ports,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
};
struct Graph {
    links: RefCell<HashSet<(String, String)>>,
    fail: Cell<usize>,
    calls: Cell<usize>,
    fail_all: bool,
}
impl PortGraph for Graph {
    fn linked(&self, from: &str, to: &str) -> bool {
        self.links.borrow().contains(&(from.into(), to.into()))
    }
    fn connect(&self, from: &str, to: &str) -> Result<(), String> {
        let call = self.calls.get() + 1;
        self.calls.set(call);
        if call == self.fail.get() || (self.fail_all && call >= self.fail.get()) {
            return Err("Injected connect failure".into());
        }
        self.links.borrow_mut().insert((from.into(), to.into()));
        Ok(())
    }
    fn disconnect(&self, from: &str, to: &str) -> Result<(), String> {
        self.links.borrow_mut().remove(&(from.into(), to.into()));
        Ok(())
    }
    fn unique(&self, owned: &str) -> bool {
        self.links
            .borrow()
            .iter()
            .filter(|(a, b)| a == owned || b == owned)
            .count()
            == 1
    }
}
fn setup() -> (Graph, Ports, Ports) {
    let old = Ports {
        inputs: [Some("old:send".into()), None, None, None],
        outputs: [Some("old:return".into()), None, None, None],
    };
    let new = Ports {
        inputs: [Some("new:send".into()), None, None, None],
        outputs: [Some("new:return".into()), None, None, None],
    };
    let mut links = connections(&old).into_iter().collect::<HashSet<_>>();
    links.insert(("other:out".into(), "other:in".into()));
    (
        Graph {
            links: RefCell::new(links),
            fail: Cell::new(0),
            calls: Cell::new(0),
            fail_all: false,
        },
        old,
        new,
    )
}
#[test]
fn successful_apply_changes_only_owned_exact_pairs() {
    let (graph, old, new) = setup();
    let edit = edit_connections(&graph, &old, &new, || Ok(()));
    assert!(edit.error.is_none());
    for (a, b) in connections(&new) {
        assert!(graph.linked(&a, &b));
    }
    for (a, b) in connections(&old) {
        assert!(!graph.linked(&a, &b));
    }
    assert!(graph.linked("other:out", "other:in"));
}
#[test]
fn connect_and_save_failures_restore_the_exact_old_graph() {
    for fail_connect in [true, false] {
        let (graph, old, new) = setup();
        let before = graph.links.borrow().clone();
        if fail_connect {
            graph.fail.set(2);
        }
        let edit = edit_connections(&graph, &old, &new, || Err("Injected save failure".into()));
        assert!(edit.error.is_some());
        assert!(!edit.rollback_failed);
        assert_eq!(*graph.links.borrow(), before);
    }
}
#[test]
fn extra_connections_are_preserved_and_rejected() {
    let (graph, old, new) = setup();
    graph
        .links
        .borrow_mut()
        .insert(("foreign:out".into(), "fx:in_1".into()));
    let before = graph.links.borrow().clone();
    let edit = edit_connections(&graph, &old, &new, || {
        panic!("must not persist an ambiguous graph")
    });
    assert!(edit.error.unwrap().contains("Extra"));
    assert!(!edit.rollback_failed);
    assert_eq!(*graph.links.borrow(), before);
}
#[test]
fn rollback_failure_is_distinct_so_owner_keeps_audio_gated() {
    let (mut graph, old, new) = setup();
    graph.fail_all = true;
    graph.fail.set(1);
    let edit = edit_connections(&graph, &old, &new, || Ok(()));
    assert!(edit.error.is_some());
    assert!(edit.rollback_failed);
    assert!(graph.linked("other:out", "other:in"));
}
