//! External consumers can create restricted scopes, never a local scope.
use mg_live::access::{ClientGroup, DeliveryMode, Scope};

#[test]
fn restricted_constructor_never_grants_unrestricted_privileges() {
    use ClientGroup::*;
    let all = [Read, ReadChat, Variables, Npc, Speech, Spawn, Scripts, Chunks, Players];
    for mask in 0..(1 << all.len()) {
        let groups: Vec<_> = all
            .iter()
            .enumerate()
            .filter_map(|(i, &group)| (mask & (1 << i) != 0).then_some(group))
            .collect();
        for delivery in [DeliveryMode::Auto, DeliveryMode::Drafts] {
            for name in ["", "default", "local", "client"] {
                let scope = Scope::restricted(name, groups.clone(), delivery);
                assert!(!scope.unrestricted());
                assert!(!scope.clone().unrestricted());
                assert_eq!(scope.name(), name);
                for group in all {
                    assert_eq!(scope.has(group), groups.contains(&group));
                }
                assert!(!scope.allows("unknown_operation"));
            }
        }
    }
}

#[test]
fn scoped_entry_points_are_available_to_external_consumers() {
    use mg_live::{events::Query, operations, store::LiveStore};
    let scope = Scope::restricted("reader", vec![ClientGroup::Read], DeliveryMode::Auto);
    let store = LiveStore::default();
    assert!(store.status_scoped(1, &scope).is_object());
    assert!(store.events_scoped(&Query::default(), &scope).0.is_empty());
    let stop = std::sync::atomic::AtomicBool::new(false);
    assert!(
        operations::call_scoped("live_status", serde_json::json!({}), &store, &stop, &scope)
            .is_ok()
    );
    assert!(operations::command_scoped(serde_json::json!({}), &store, 1, &scope).is_err());
    // Resolve the method from outside the crate without relying on authorization
    // or command-queue behavior, which have their own store regressions.
    let _enqueue = LiveStore::enqueue_scoped;
}
