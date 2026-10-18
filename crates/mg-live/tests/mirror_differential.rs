use mg_core::ObjectKind;
use mg_live::{
    gateway::{Gateway, Push, Resume, SessionConfig},
    gateway_frames::{DecodedServer, ServerFrame, decode_server},
    store::{Command, ErrorCode, GitObject, LiveStore, reconcile},
};
use proptest::{
    prelude::*,
    test_runner::{Config, RngAlgorithm, TestRng, TestRunner},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};
fn config() -> SessionConfig {
    SessionConfig {
        id: "one".into(),
        id_prefix: "one.".into(),
        groups: ["read", "read_chat", "read_tells", "npc"].into_iter().map(str::to_owned).collect(),
        watch_limit: 16,
    }
}
struct Pair {
    authority: Arc<LiveStore>,
    mirror: LiveStore,
    gateway: Gateway,
    revs: BTreeMap<String, u64>,
    now: u64,
    linked: bool,
    ids: Vec<String>,
}
impl Pair {
    fn new() -> Self {
        let authority = Arc::new(LiveStore::default());
        let gateway = Gateway::new(authority.clone()).unwrap();
        let mirror = LiveStore::new_mirror();
        mirror.set_id_prefix("one.").unwrap();
        gateway.connect(config(), None, &["square".into()], 1000).unwrap();
        Self {
            authority,
            mirror,
            gateway,
            revs: BTreeMap::new(),
            now: 1000,
            linked: true,
            ids: vec![],
        }
    }
    fn feed(&mut self, frame: Value) {
        self.now += 1000;
        self.authority.sync(&serde_json::to_vec(&frame).unwrap(), self.now).unwrap();
        if self.linked {
            self.deliver();
            self.equal();
        }
    }
    fn forget_completed_results(&mut self) {
        // Reset intentionally releases completed mirror history. Fresh result
        // pushes below add retained outbox results back to the comparison set.
        self.ids.retain(|id| {
            self.mirror.result(id).is_none_or(|r| r.pending || r.code == Some(ErrorCode::Unknown))
        });
    }
    fn deliver(&mut self) {
        self.gateway.tick(self.now).unwrap();
        for _ in 0..100 {
            for push in self.gateway.pushes("one", self.now).unwrap() {
                // The mirror receives only serialized gateway pushes, never reports or store copies.
                let bytes = serde_json::to_vec(&ServerFrame::Push(push)).unwrap();
                let DecodedServer::Frame(frame) = decode_server(&bytes).unwrap() else {
                    panic!("Known push skipped")
                };
                let ServerFrame::Push(push) = *frame else { panic!("Expected push") };
                match &push {
                    Push::Area(a) => {
                        self.revs.insert(a.area.clone(), a.rev);
                    }
                    Push::AreaGone { area, .. } => {
                        self.revs.remove(area);
                    }
                    Push::Boot { .. } => {
                        self.revs.clear();
                        self.forget_completed_results();
                    }
                    Push::Result(r)
                        if self.authority.result(&r.id).is_some() && !self.ids.contains(&r.id) =>
                    {
                        self.ids.push(r.id.clone());
                    }
                    _ => {}
                }
                push.apply(&self.mirror, self.now).unwrap();
            }
            if self.mirror.event_cursor() >= self.authority.event_cursor() {
                return;
            }
        }
        panic!("bounded event batches did not catch up");
    }
    fn equal(&self) {
        assert_eq!(self.authority.area("square"), self.mirror.area("square"));
        assert_eq!(self.authority.find("", 200), self.mirror.find("", 200));
        for id in ["a1", "a2", "missing"] {
            assert_eq!(self.authority.locate(id), self.mirror.locate(id));
        }
        assert_eq!(
            self.authority.events(&Default::default()),
            self.mirror.events(&Default::default())
        );
        for id in &self.ids {
            assert_eq!(self.authority.result(id), self.mirror.result(id), "result {id}");
        }
        let git = [
            GitObject {
                kind: ObjectKind::Placeable,
                template: "barrel",
                position: glam::vec3(10., 10.05, 0.),
                is_static: false,
            },
            GitObject {
                kind: ObjectKind::Creature,
                template: "guard",
                position: glam::Vec3::ZERO,
                is_static: false,
            },
        ];
        if let (Some(a), Some(b)) = (self.authority.area("square"), self.mirror.area("square")) {
            assert_eq!(reconcile(&git, &a), reconcile(&git, &b));
        }
    }
    fn reconnect(&mut self) {
        let resume = Resume {
            gw_epoch: self.gateway.epoch().into(),
            boot: self.mirror.boot(),
            event_n: self.mirror.event_cursor(),
            areas: self.revs.clone(),
        };
        let resumed =
            self.gateway.connect(config(), Some(&resume), &["square".into()], self.now).unwrap();
        if !resumed {
            self.forget_completed_results();
            self.mirror.mirror_reset("resume refused").unwrap();
            self.revs.clear();
        }
        self.linked = true;
        self.deliver();
        self.equal();
    }
    fn restart_gateway(&mut self) {
        if self.mirror.boot().is_empty() {
            return;
        }
        let command: Command =
            serde_json::from_value(json!({"op":"stop_patrol","area":"square","target":"a1"}))
                .unwrap();
        let id = self.mirror.enqueue(command, self.now).unwrap();
        let resume = Resume {
            gw_epoch: self.gateway.epoch().into(),
            boot: self.mirror.boot(),
            event_n: self.mirror.event_cursor(),
            areas: self.revs.clone(),
        };
        self.gateway = Gateway::new(self.authority.clone()).unwrap();
        assert!(
            !self.gateway.connect(config(), Some(&resume), &["square".into()], self.now).unwrap()
        );
        self.forget_completed_results();
        self.mirror.mirror_reset("gateway epoch changed").unwrap();
        self.revs.clear();
        assert_eq!(self.mirror.result(&id).unwrap().code, Some(ErrorCode::Unknown));
        assert!(self.mirror.mirror_commands(self.now).unwrap().is_empty());
        self.linked = true;
        self.deliver();
        self.equal();
    }
}
fn report(boot: &str, seq: u64, base: Option<u64>, x: u8) -> Value {
    let mut value = json!({"v":1,"boot":boot,"seq":seq,"areas":[{"area":"square","full":base.is_none(),"cre":[{"id":"a1","r":"guard","name":"Guard","x":x,"hp":[x,255]}],"plc":[{"id":"a2","r":"barrel","x":10,"y":10.05}]}]});
    if let Some(base) = base {
        value["base_seq"] = json!(base);
    }
    value
}
#[test]
fn captured_store_regressions_replay_through_gateway_frames() {
    let mut pair = Pair::new();
    let frames: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/mirror-reports.json")).unwrap();
    for frame in frames {
        pair.feed(frame);
    }
    pair.linked = false;
    pair.feed(report("new", 2, None, 8));
    pair.reconnect();
    pair.restart_gateway();
}
#[test]
fn differential_random_reports_restarts_gaps_and_resume_fixed_seeds() {
    for seed in [7, 19, 43, 91] {
        let mut runner = TestRunner::new_with_rng(
            Config { cases: 48, failure_persistence: None, ..Config::default() },
            TestRng::from_seed(RngAlgorithm::ChaCha, &[seed; 32]),
        );
        runner.run(&proptest::collection::vec(any::<u8>(),1..64),|steps| {
        let mut pair=Pair::new();let mut boot=0;pair.feed(report("boot0",1,None,0));
        for step in steps {
            let seq=pair.authority.revision().1;let name=format!("boot{boot}");
            match step%10 {
                0|1=>pair.feed(report(&name,seq+1,None,step)),
                2=>pair.feed(report(&name,seq+1,Some(seq),step)),
                3=>pair.feed(report(&name,seq+2,Some(seq),step)),
                4=>pair.feed(report(&name,seq.saturating_sub(1),None,step)),
                5=>{pair.linked=false;pair.feed(report(&name,seq+1,None,step));},
                6=>pair.reconnect(),
                7=>{boot+=1;pair.feed(report(&format!("boot{boot}"),1,None,step));},
                8=>pair.restart_gateway(),
                _=>{
                    pair.reconnect();
                    let id=format!("one.abcdef12.{}",pair.now);
                    let reply=pair.gateway.commands("one",&[json!({"id":id,"boot":name,"op":"stop_patrol","area":"square","target":"a1"})],pair.now).unwrap();
                    if !reply.accepted.is_empty() {pair.ids.push(id.clone());let mut frame=report(&name,seq+1,None,step);frame["results"]=json!([{"id":id,"ok":true}]);pair.feed(frame);}
                }
            }
        }
        pair.reconnect();Ok(())
    }).unwrap();
    }
}

#[test]
fn pending_result_survives_boot_reset_without_local_revision_collision() {
    let mut pair = Pair::new();
    pair.feed(report("boot0", 1, None, 1));
    let command: Command =
        serde_json::from_value(json!({"op":"stop_patrol","area":"square","target":"a1"})).unwrap();
    let id = pair.mirror.enqueue(command, pair.now).unwrap();
    let envelope =
        serde_json::to_value(&pair.mirror.mirror_commands(pair.now).unwrap()[0]).unwrap();
    assert!(pair.gateway.commands("one", &[envelope], pair.now).unwrap().rejected.is_empty());
    pair.mirror.mirror_mark_sent(std::slice::from_ref(&id)).unwrap();
    pair.deliver();
    // Raise UI revision well above the command's result revision.
    for n in 0..20 {
        pair.mirror.mirror_events(vec![], n).unwrap();
    }
    pair.ids.push(id);
    pair.feed(report("new-boot", 1, None, 1));
}

#[test]
fn player_areas_share_watch_capacity_without_failing_session_delivery() {
    let pair = Pair::new();
    let areas:Vec<_>=(0..64).map(|i|json!({"area":format!("pc{i}"),"full":true,"cre":[{"id":format!("{i:x}"),"pc":true}]})).collect();
    pair.authority
        .sync(
            &serde_json::to_vec(&json!({"v":1,"boot":"boot","seq":1,"areas":areas})).unwrap(),
            1000,
        )
        .unwrap();
    let reply = pair.gateway.watch_set("one", &["square".into()], 1000).unwrap();
    assert_eq!(reply.waiting, ["square"]);
    assert_eq!(
        pair.gateway
            .pushes("one", 1000)
            .unwrap()
            .iter()
            .filter(|p| matches!(p, Push::Area(_)))
            .count(),
        64
    );
}

#[test]
fn meta_revision_is_shared_and_acked_results_are_not_recreated_by_retries() {
    let mut pair = Pair::new();
    pair.feed(report("boot0", 1, None, 1));
    let mut second = config();
    second.id = "two".into();
    second.id_prefix = "two.".into();
    pair.gateway.connect(second, None, &[], pair.now).unwrap();
    let first = pair.gateway.pushes("one", pair.now + 1).unwrap();
    let second = pair.gateway.pushes("two", pair.now + 1).unwrap();
    let rev = |pushes: Vec<Push>| {
        pushes
            .into_iter()
            .find_map(|p| if let Push::Meta(m) = p { Some(m.rev) } else { None })
            .unwrap()
    };
    assert_eq!(rev(first), rev(second));
    let command = json!({"id":"one.1234abcd.1","boot":"boot0","op":"stop_patrol","area":"square","target":"a1"});
    pair.gateway.commands("one", std::slice::from_ref(&command), pair.now).unwrap();
    let mut frame = report("boot0", 2, None, 1);
    frame["results"] = json!([{"id":"one.1234abcd.1","ok":true}]);
    pair.feed(frame);
    pair.gateway.ack("one", &[("one.1234abcd.1".into(), 2)], pair.mirror.event_cursor()).unwrap();
    pair.gateway.commands("one", &[command], pair.now).unwrap();
    assert!(
        !pair.gateway.pushes("one", pair.now).unwrap().iter().any(|p| matches!(p, Push::Result(_)))
    );
}

#[test]
fn retention_gap_refuses_resume_and_recovers_the_same_event_tail() {
    let mut pair = Pair::new();
    pair.feed(report("boot0", 1, None, 0));
    pair.linked = false;
    for batch in 0..2 {
        let mut frame = report("boot0", batch + 2, None, 1);
        frame["events"]=json!((1..=700).map(|n|json!({"seq":batch*700+n,"kind":"chat","area":"square","channel":1,"text":"retained"})).collect::<Vec<_>>());
        pair.feed(frame);
    }
    let resume = Resume {
        gw_epoch: pair.gateway.epoch().into(),
        boot: pair.mirror.boot(),
        event_n: pair.mirror.event_cursor(),
        areas: pair.revs.clone(),
    };
    assert!(!pair.gateway.connect(config(), Some(&resume), &["square".into()], pair.now).unwrap());
    pair.mirror.mirror_reset("retention gap").unwrap();
    pair.revs.clear();
    pair.linked = true;
    pair.deliver();
    pair.equal();
}

#[test]
fn result_delivered_during_link_drop_is_replayed_on_resume() {
    let mut pair = Pair::new();
    pair.feed(report("boot0", 1, None, 1));
    let id = "one.1234abcd.1";
    pair.gateway
        .commands(
            "one",
            &[json!({"id":id,"boot":"boot0","op":"stop_patrol","area":"square","target":"a1"})],
            pair.now,
        )
        .unwrap();
    pair.ids.push(id.into());
    pair.deliver();
    pair.linked = false;
    let mut frame = report("boot0", 2, None, 2);
    frame["results"] = json!([{"id":id,"ok":true}]);
    pair.feed(frame);
    pair.reconnect();
    assert!(pair.mirror.result(id).unwrap().ok);
}

#[test]
fn arbitrary_bytes_never_panic_frame_decoders() {
    let mut runner = TestRunner::new_with_rng(
        Config { cases: 128, failure_persistence: None, ..Config::default() },
        TestRng::from_seed(RngAlgorithm::ChaCha, &[23; 32]),
    );
    runner
        .run(&proptest::collection::vec(any::<u8>(), 0..4096), |bytes| {
            let _ = mg_live::gateway_frames::decode_client(&bytes);
            let _ = decode_server(&bytes);
            Ok(())
        })
        .unwrap();
}

#[test]
fn mirror_status_keeps_local_command_ownership() {
    use mg_live::access::{ClientGroup, DeliveryMode, Scope};
    let mut pair = Pair::new();
    pair.feed(report("boot0", 1, None, 0));
    let a = Scope::restricted("a", vec![ClientGroup::Npc], DeliveryMode::Auto);
    let b = Scope::restricted("b", vec![ClientGroup::Npc], DeliveryMode::Auto);
    let command = || {
        serde_json::from_value(json!({"op":"stop_patrol","area":"square","target":"a1"})).unwrap()
    };
    let aid = pair.mirror.enqueue_scoped(command(), pair.now, &a).unwrap();
    let bid = pair.mirror.enqueue_scoped(command(), pair.now, &b).unwrap();
    assert_eq!(pair.mirror.status_scoped(pair.now, &a)["queued"].as_array().unwrap().len(), 1);
    pair.mirror.mirror_reject(&aid, ErrorCode::Forbidden, "denied").unwrap();
    pair.mirror.mirror_reject(&bid, ErrorCode::Forbidden, "denied").unwrap();
    let status = pair.mirror.status_scoped(pair.now, &a);
    assert_eq!(status["results"].as_array().unwrap().len(), 1);
    assert_eq!(status["results"][0]["result"]["id"], aid);
}

#[test]
fn explicit_null_response_is_present_and_round_trips() {
    let response =
        ServerFrame::Res(mg_live::gateway_frames::Response { id: 1, result: Ok(Value::Null) });
    let DecodedServer::Frame(decoded) =
        decode_server(&serde_json::to_vec(&response).unwrap()).unwrap()
    else {
        panic!("Response skipped")
    };
    assert_eq!(*decoded, response);
}

#[test]
fn malformed_focus_cannot_poison_other_sessions_watch_union() {
    let mut pair = Pair::new();
    pair.feed(report("boot0", 1, None, 0));
    let mut cfg = config();
    cfg.id = "focus".into();
    cfg.id_prefix = "focus.".into();
    cfg.groups.push("focus".into());
    pair.gateway.connect(cfg, None, &[], pair.now).unwrap();
    assert!(pair.gateway.focus_set("focus", vec![("invalid/area".into(), "a1".into())]).is_err());
    assert!(pair.gateway.focus_set("focus", vec![("square".into(), "bad-id".into())]).is_err());
    pair.deliver();
    pair.equal();
}

#[test]
fn overlapping_watch_owners_do_not_lose_reserved_capacity() {
    let pair = Pair::new();
    pair.authority.set_watched_areas_owned("local", &["square".into()], 1000).unwrap();
    let mut cfg = config();
    cfg.watch_limit = 64;
    pair.gateway.connect(cfg, None, &["square".into()], 1000).unwrap();
    let areas = (0..64).map(|n| format!("area{n}")).collect::<Vec<_>>();
    let reply = pair.gateway.watch_set("one", &areas, 1000).unwrap();
    assert_eq!(reply.accepted.len(), 63);
    assert_eq!(reply.waiting.len(), 1);
    assert!(pair.gateway.pushes("one", 1000).is_ok());
}
