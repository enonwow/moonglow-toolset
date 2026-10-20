//! Drives a running Live gateway with the desktop's own link client, end to end:
//! connect, watch an area, read its creatures, make one NPC speak and wait for the
//! game's result.
//!
//!     MOONGLOW_GATEWAY_URL=http://127.0.0.1:5199 MOONGLOW_GATEWAY_TOKEN=... \
//!         cargo run -p mg-live-link --example live_probe -- <area>
use mg_live_link::credentials::{Backend, StoreError, TokenStore};
use mg_live_link::{Gateway, LinkConfig, LinkService, LinkStatus, SessionToken};
use std::time::{Duration, Instant};

struct Env(SessionToken);
impl Backend for Env {
    fn get(&self, _: &str) -> Result<Option<SessionToken>, StoreError> {
        Ok(Some(self.0.clone()))
    }
    fn set(&self, _: &str, _: &SessionToken) -> Result<(), StoreError> {
        Ok(())
    }
    fn delete(&self, _: &str) -> Result<(), StoreError> {
        Ok(())
    }
}

fn until<T>(what: &str, seconds: u64, mut f: impl FnMut() -> Option<T>) -> T {
    let end = Instant::now() + Duration::from_secs(seconds);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn main() {
    let url = std::env::var("MOONGLOW_GATEWAY_URL").expect("MOONGLOW_GATEWAY_URL");
    let token = SessionToken::new(std::env::var("MOONGLOW_GATEWAY_TOKEN").expect("token")).unwrap();
    let area = std::env::args().nth(1).expect("area resref");
    let tokens = TokenStore::new(Env(token), std::path::Path::new("unused"));
    let config = LinkConfig::new("probe", Gateway::new(&url, None).unwrap());
    let started = Instant::now();
    let link = LinkService::start(config, &tokens, std::sync::Arc::new(|| {})).unwrap();
    until("connection", 20, || (link.snapshot().status == LinkStatus::Connected).then_some(()));
    let welcome = link.snapshot().welcome.unwrap();
    println!("connected in {:?} as {}", started.elapsed(), welcome.user.id);
    if std::env::var_os("PROBE_DRAFTS").is_some() {
        std::thread::sleep(Duration::from_secs(2));
        let raw = link.snapshot().drafts.len();
        println!("drafts pushed: {raw}, revision {}", link.draft_revision());
        match link.drafts() {
            Ok(d) => println!("drafts read: {}", d.len()),
            Err(e) => println!("drafts unreadable: {e:?}"),
        }
        if let Some(first) = link.snapshot().drafts.first() {
            println!("first draft json: {}", serde_json::to_string(first).unwrap_or_default());
        }
        return;
    }

    link.set_watched_areas(std::slice::from_ref(&area))
        .unwrap()
        .wait(Duration::from_secs(10))
        .unwrap();
    let live =
        until("area report", 30, || link.store().area(&area).filter(|a| !a.creatures.is_empty()));
    let npcs: Vec<_> = live.creatures.iter().filter(|c| !c.pc && !c.dead).collect();
    println!("{area}: {} creatures, {} living NPCs", live.creatures.len(), npcs.len());
    for c in npcs.iter().take(8) {
        println!("  {} ({}) at {:.1},{:.1}", c.name, c.id, c.x, c.y);
    }
    let npc = npcs.first().expect("a living NPC");

    let sent = Instant::now();
    let command = serde_json::from_value(serde_json::json!({
        "op": "npc_say", "area": area, "target": npc.id, "text": "Moonglow probe: hello from the gateway"
    }))
    .unwrap();
    let id = link.enqueue(command).unwrap();
    let result = until("command result", 30, || link.store().result(&id).filter(|r| !r.pending));
    println!("npc_say {id}: ok={} err={:?} in {:?}", result.ok, result.err, sent.elapsed());
    assert!(result.ok, "the game refused the command");
    link.stop();
}
