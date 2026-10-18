use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Default)]
struct Fake {
    calls: AtomicUsize,
    delay: u64,
    output: Option<String>,
}
impl Translator for Fake {
    fn translate<'a>(
        &'a self,
        text: &'a str,
        _from: &'a str,
        to: &'a str,
    ) -> TranslationFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(self.delay)).await;
            Ok(self.output.clone().unwrap_or_else(|| format!("{text} {to}")))
        })
    }
}
fn value(text: &str) -> Value {
    json!({"req":"boot:1","slot":1,"text":text,"from":"en","to":{"pl":true,"de":true},"player":"player1","channel":1})
}
fn fixture(fake: Arc<Fake>) -> (TranslationPipeline, PathBuf) {
    let path =
        std::env::temp_dir().join(format!("mg-xl-{}", mg_live::service::generate_token().unwrap()));
    let pipeline = TranslationPipeline::new(
        fake,
        TranslationConfig { enabled: true, ..Default::default() },
        path.join("languages.json"),
    )
    .unwrap();
    (pipeline, path)
}
async fn answer(p: &TranslationPipeline, v: Value) -> Value {
    let (send, receive) = tokio::sync::oneshot::channel();
    p.request(
        v,
        Box::new(move |v| {
            let _ = send.send(v);
        }),
    );
    tokio::time::timeout(Duration::from_secs(2), receive).await.unwrap().unwrap()
}
#[tokio::test]
async fn segments_preserve_markup_and_cache_by_language_pair() {
    let fake = Arc::new(Fake::default());
    let (p, _) = fixture(fake.clone());
    let a = answer(&p, value("*smiles* <cFF0000>hello</c>")).await;
    assert_eq!(a["to"]["pl"], "*smiles pl* <cFF0000>hello pl</c>");
    assert_eq!(fake.calls.load(Ordering::SeqCst), 4);
    assert_eq!(answer(&p, value("*smiles* <cFF0000>hello</c>")).await, a);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 4);
    let mut v = value("*smiles* <cFF0000>hello</c>");
    v["from"] = json!("fr");
    answer(&p, v).await;
    assert_eq!(fake.calls.load(Ordering::SeqCst), 8);
}
#[tokio::test]
async fn engine_deadline_and_languages_run_in_parallel() {
    let (p, _) = fixture(Arc::new(Fake { delay: 400, ..Default::default() }));
    let start = Instant::now();
    let result = answer(&p, value("hello")).await;
    assert!(result["to"].is_object());
    assert!(start.elapsed() < Duration::from_millis(650));
    let (p, _) = fixture(Arc::new(Fake { delay: 900, ..Default::default() }));
    let start = Instant::now();
    assert_eq!(answer(&p, value("hello")).await["miss"], true);
    assert!(start.elapsed() < Duration::from_millis(850));
}
#[tokio::test]
async fn rejects_commands_and_untrusted_outputs() {
    for output in [
        " /destroy",
        " !danger",
        "<cFF0000>oops</c>",
        "*oops*",
        "line\nforged",
        "",
        &"x".repeat(200),
    ] {
        let (p, _) = fixture(Arc::new(Fake { output: Some(output.into()), ..Default::default() }));
        assert_eq!(answer(&p, value("hello")).await["miss"], true, "{output}");
    }
    let fake = Arc::new(Fake::default());
    let (p, _) = fixture(fake.clone());
    p.configure(TranslationConfig {
        enabled: true,
        command_prefixes: vec!["$".into()],
        ..Default::default()
    });
    for input in [" /command", "!command", "$command", "<bad>hello</bad>", "*open"] {
        assert_eq!(answer(&p, value(input)).await["miss"], true);
    }
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn rate_is_per_player_and_config_can_disable_channels() {
    let (p, _) = fixture(Arc::new(Fake::default()));
    p.configure(TranslationConfig {
        enabled: true,
        per_player_per_minute: 1,
        ..Default::default()
    });
    assert!(answer(&p, value("hello")).await["to"].is_object());
    assert_eq!(answer(&p, value("hello")).await["miss"], true);
    let mut other = value("hello");
    other["player"] = json!("player2");
    assert!(answer(&p, other).await["to"].is_object());
    p.configure(TranslationConfig { enabled: true, channels: vec![4], ..Default::default() });
    assert_eq!(answer(&p, value("hello")).await["miss"], true);
}
#[test]
fn priority_expiry_rate_and_lru_are_bounded() {
    let mut state = State::default();
    let now = Instant::now();
    for channel in [2, 6, 4, 1] {
        let mut v = value("hello");
        v["channel"] = json!(channel);
        state.queue.push(Job {
            request: serde_json::from_value(v).unwrap(),
            deadline: now,
            reply: Box::new(|_| {}),
        });
    }
    assert_eq!(state.pop().unwrap().request.channel, 4);
    assert_eq!(state.pop().unwrap().request.channel, 1);
    assert_eq!(state.pop().unwrap().request.channel, 6);
    assert_eq!(state.pop().unwrap().request.channel, 2);
    assert!(state.admit("one", 1, now));
    assert!(!state.admit("one", 1, now));
    assert!(state.admit("one", 1, now + Duration::from_secs(60)));
    for i in 0..2000 {
        state.cache((i.to_string(), "en".into(), "pl".into()), "x".repeat(1024));
    }
    assert!(state.cache.len() <= 1024 && state.cache_bytes <= 1024 * 1024);
    assert!(state.cached(&("0".into(), "en".into(), "pl".into())).is_none());
    assert!(state.cached(&("1999".into(), "en".into(), "pl".into())).is_some());
}
#[tokio::test]
async fn expired_queued_job_never_calls_engine() {
    let fake = Arc::new(Fake::default());
    let (p, _) = fixture(fake.clone());
    let (send, receive) = tokio::sync::oneshot::channel();
    p.state.lock().unwrap().queue.push(Job {
        request: serde_json::from_value(value("hello")).unwrap(),
        deadline: Instant::now() - Duration::from_secs(1),
        reply: Box::new(move |v| {
            let _ = send.send(v);
        }),
    });
    p.notify.notify_one();
    assert_eq!(receive.await.unwrap()["miss"], true);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn language_survives_restart_and_accepts_only_supported_languages() {
    let (p, path) = fixture(Arc::new(Fake::default()));
    let request = json!({"kind":"language","req":"boot:join1","pc":"12345678","player":"PUBLICKEY","lang":"pl"});
    assert_eq!(answer(&p, request).await["lang"], "pl");
    drop(p);
    let p = TranslationPipeline::new(
        Arc::new(Fake::default()),
        Default::default(),
        path.join("languages.json"),
    )
    .unwrap();
    let request =
        json!({"kind":"language","req":"boot:join2","pc":"87654321","player":"PUBLICKEY"});
    let reply = answer(&p, request).await;
    assert_eq!(reply["lang"], "pl");
    assert_eq!(reply["req"], "boot:join2");
    assert!(!LANGUAGES.contains(&"zh"));
    drop(p);
    std::fs::remove_dir_all(path).unwrap();
}

#[tokio::test]
async fn disabling_in_flight_translation_returns_original() {
    let (p, _) = fixture(Arc::new(Fake { delay: 100, ..Default::default() }));
    let (send, receive) = tokio::sync::oneshot::channel();
    p.request(
        value("hello"),
        Box::new(move |v| {
            let _ = send.send(v);
        }),
    );
    tokio::time::sleep(Duration::from_millis(20)).await;
    p.configure(TranslationConfig::default());
    assert_eq!(receive.await.unwrap()["miss"], true);
    assert_eq!(answer(&p, value("hello")).await["miss"], true);
}

#[tokio::test]
async fn reversed_language_workers_cannot_overwrite_a_newer_choice() {
    let (p, path) = fixture(Arc::new(Fake::default()));
    // Worker 2 reaches storage first; worker 1 was delayed in the blocking pool.
    assert!(p.languages.save("PLAYER", "pl", 2).unwrap());
    assert!(!p.languages.save("PLAYER", "de", 1).unwrap());
    // Revision order is per player, not a global drop of other players' writes.
    assert!(p.languages.save("OTHER", "fr", 1).unwrap());
    assert!(p.languages.save("PLAYER", "it", 3).unwrap());
    drop(p);
    let restored = LanguageBook::open(path.join("languages.json")).unwrap();
    assert_eq!(restored.values.lock().unwrap()["PLAYER"], "it");
    assert_eq!(restored.values.lock().unwrap()["OTHER"], "fr");
    drop(restored);
    std::fs::remove_dir_all(path).unwrap();
}
