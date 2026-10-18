//! Fake engine for tools/live/test_chat_translation.py, through the production bridge.
use mg_gateway::{
    TranslationConfig, TranslationPipeline, Translator, translation::TranslationFuture,
};
use mg_live::{
    bridge::{Bridge, RedisConfig},
    store::LiveStore,
};
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

#[derive(Debug)]
struct FakeTranslator;
impl Translator for FakeTranslator {
    fn translate<'a>(
        &'a self,
        text: &'a str,
        _from: &'a str,
        to: &'a str,
    ) -> TranslationFuture<'a> {
        Box::pin(async move {
            let delay = if text.starts_with("xl_drop") || text.starts_with("xl_late") {
                1200
            } else if text.starts_with("xl_move") {
                600
            } else if text.starts_with("xl_b") {
                text.split('_')
                    .nth(1)
                    .and_then(|s| s.strip_prefix('b'))
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0)
            } else {
                0
            };
            tokio::time::sleep(Duration::from_millis(delay)).await;
            if text.starts_with("xl_command") {
                return Ok(" /danger".into());
            }
            if text.starts_with("xl_markup") {
                return Ok("<bad>changed</bad>".into());
            }
            Ok(format!("{text} translated-{to}"))
        })
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::temp_dir().join(format!("mg-xl-engine-{}", std::process::id()));
    let store = Arc::new(LiveStore::default());
    let pipeline = Arc::new(TranslationPipeline::new(
        Arc::new(FakeTranslator),
        TranslationConfig { enabled: true, per_player_per_minute: 1000, ..Default::default() },
        data.join("languages.json"),
    )?);
    store.set_translation_hook(Some(pipeline));
    let bridge = Bridge::start(
        RedisConfig { host: "127.0.0.1".into(), port: 6392, prefix: "mglive-events".into() },
        store,
        Arc::new(|| {}),
        Arc::new(AtomicBool::new(false)),
    )?;
    let until = std::time::Instant::now() + Duration::from_secs(10);
    while bridge.status()["connected"] != true {
        if std::time::Instant::now() >= until {
            return Err("bridge not ready".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    println!("translation bridge ready");
    tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)
    })
    .await??;
    drop(bridge);
    if data.exists() {
        std::fs::remove_dir_all(data)?;
    }
    Ok(())
}
