//! Concurrent-use counters per principal, so one user cannot take a shared pool alone.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Debug, Default)]
pub(crate) struct PerPrincipal(Mutex<HashMap<String, usize>>);
/// Returns its place on drop.
#[derive(Debug)]
pub(crate) struct Slot {
    owner: Arc<PerPrincipal>,
    id: String,
}
impl PerPrincipal {
    pub(crate) fn acquire(self: &Arc<Self>, id: &str, limit: usize) -> Option<Slot> {
        let mut counts = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let used = counts.entry(id.to_owned()).or_default();
        if *used >= limit {
            return None;
        }
        *used += 1;
        Some(Slot { owner: self.clone(), id: id.to_owned() })
    }
}
impl Drop for Slot {
    fn drop(&mut self) {
        let mut counts = self.owner.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(used) = counts.get_mut(&self.id) {
            *used -= 1;
            if *used == 0 {
                counts.remove(&self.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_principal_is_capped_others_are_not_and_slots_return_on_drop() {
        let slots = Arc::new(PerPrincipal::default());
        let a: Vec<_> = (0..2).map(|_| slots.acquire("a", 2).unwrap()).collect();
        assert!(slots.acquire("a", 2).is_none());
        let b = slots.acquire("b", 2).unwrap();
        drop(a);
        let again = slots.acquire("a", 2);
        assert!(again.is_some());
        drop(b);
        assert_eq!(slots.0.lock().unwrap().len(), 1, "only a's entry is left");
        drop(again);
        assert!(slots.0.lock().unwrap().is_empty());
    }
}
