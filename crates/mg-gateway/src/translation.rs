//! Bounded, opt-in translation. The game owns audiences and exactly-once fallback.
use futures_util::future::join_all;
use mg_live::bridge::{TranslationHook, TranslationReply};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fmt::Debug,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub type TranslationFuture<'a> = Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;
/// Implementations must be cancellation-safe and must not block the executor.
/// Text contains no emote or colour delimiters; credentials stay in the host.
pub trait Translator: Debug + Send + Sync + 'static {
    fn translate<'a>(&'a self, text: &'a str, from: &'a str, to: &'a str) -> TranslationFuture<'a>;
}
pub const LANGUAGES: &[&str] = &["en", "pl", "de", "fr", "es", "it"];
const DEADLINE: Duration = Duration::from_millis(650);
const QUEUE: usize = 128;
const CONCURRENCY: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationConfig {
    pub enabled: bool,
    pub channels: Vec<u32>,
    pub per_player_per_minute: u32,
    pub command_prefixes: Vec<String>,
}
impl Default for TranslationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            channels: vec![1, 2, 3, 4, 6, 14, 17, 18, 19, 20, 22, 30],
            per_player_per_minute: 60,
            command_prefixes: vec!["/".into(), "!".into()],
        }
    }
}
impl TranslationConfig {
    fn command(&self, text: &str) -> bool {
        let text = text.trim_start();
        text.starts_with('/')
            || text.starts_with('!')
            || self.command_prefixes.iter().any(|p| !p.is_empty() && text.starts_with(p))
    }
}

#[derive(Debug, Deserialize)]
struct Request {
    req: String,
    slot: u32,
    text: String,
    from: String,
    to: BTreeMap<String, bool>,
    channel: u32,
    player: String,
}
struct Job {
    request: Request,
    deadline: Instant,
    reply: TranslationReply,
}
impl Job {
    fn priority(&self) -> (u8, Instant) {
        (
            match self.request.channel {
                4 | 20 | 1 | 17 => 0,
                2 | 18 => 2,
                _ => 1,
            },
            self.deadline,
        )
    }
    fn miss(self) {
        (self.reply)(miss(&self.request));
    }
}
fn miss(r: &Request) -> Value {
    json!({"req":r.req,"slot":r.slot,"miss":true})
}

#[derive(Default)]
struct State {
    queue: Vec<Job>,
    rate: HashMap<String, (Instant, u32)>,
    cache: VecDeque<((String, String, String), String)>,
    cache_bytes: usize,
}
impl State {
    fn admit(&mut self, player: &str, limit: u32, now: Instant) -> bool {
        self.rate.retain(|_, (at, _)| now.duration_since(*at) < Duration::from_secs(60));
        if !self.rate.contains_key(player) && self.rate.len() >= 4096 {
            return false;
        }
        let (_, used) = self.rate.entry(player.into()).or_insert((now, 0));
        if *used >= limit {
            return false;
        }
        *used += 1;
        true
    }
    fn pop(&mut self) -> Option<Job> {
        let i = self.queue.iter().enumerate().min_by_key(|(_, j)| j.priority())?.0;
        Some(self.queue.remove(i))
    }
    fn cached(&mut self, key: &(String, String, String)) -> Option<String> {
        let i = self.cache.iter().position(|(k, _)| k == key)?;
        let entry = self.cache.remove(i)?;
        let result = entry.1.clone();
        self.cache.push_back(entry);
        Some(result)
    }
    fn cache(&mut self, key: (String, String, String), value: String) {
        // Concurrent equal misses can finish together. Keep a single LRU entry.
        if self.cached(&key).is_some() {
            return;
        }
        let size = key.0.len() + key.1.len() + key.2.len() + value.len();
        while self.cache.len() >= 1024 || self.cache_bytes + size > 1024 * 1024 {
            let Some((k, v)) = self.cache.pop_front() else { return };
            self.cache_bytes -= k.0.len() + k.1.len() + k.2.len() + v.len();
        }
        self.cache_bytes += size;
        self.cache.push_back((key, value));
    }
}

/// Markup remains in our possession. Only plain segments cross the translator boundary.
fn segments(text: &str) -> Option<Vec<(bool, &str)>> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let mut stars = false;
    let mut colour = false;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        let len = match bytes[i] {
            b'*' => {
                stars = !stars;
                1
            }
            b'<' => {
                let end = text[i..].find('>')? + i;
                let tag = &text[i..=end];
                if tag == "</c>" && colour {
                    colour = false;
                } else if tag.starts_with("<c") && tag.len() > 3 && !colour {
                    colour = true;
                } else {
                    return None;
                }
                end - i + 1
            }
            b'>' | b'\n' | b'\r' => return None,
            _ => {
                i += 1;
                continue;
            }
        };
        if start < i {
            out.push((false, &text[start..i]));
        }
        out.push((true, &text[i..i + len]));
        i += len;
        start = i;
    }
    if stars || colour {
        return None;
    }
    if start < text.len() {
        out.push((false, &text[start..]));
    }
    (out.len() <= 64).then_some(out)
}
fn safe(config: &TranslationConfig, original: &str, output: &str) -> bool {
    !output.is_empty()
        && output.len() <= 4096
        && output.chars().count() <= 1024
        && output.len() <= original.len() * 4 + 32
        && !config.command(output)
        && segments(original)
            .zip(segments(output))
            .is_some_and(|(a, b)| a.iter().filter(|(m, _)| *m).eq(b.iter().filter(|(m, _)| *m)))
}

/// Construct in a Tokio runtime, then attach through CoreHandle::with_store.
/// Drop the last handle to stop queued work; every reply is still protected by the game timer.
pub struct TranslationPipeline {
    state: Arc<Mutex<State>>,
    config: Arc<Mutex<TranslationConfig>>,
    notify: Arc<tokio::sync::Notify>,
    worker: tokio::task::JoinHandle<()>,
    languages: Arc<LanguageBook>,
}
impl Debug for TranslationPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TranslationPipeline")
    }
}
impl Drop for TranslationPipeline {
    fn drop(&mut self) {
        self.worker.abort();
    }
}
impl TranslationPipeline {
    pub fn new(
        translator: Arc<dyn Translator>,
        config: TranslationConfig,
        languages: PathBuf,
    ) -> Result<Self, String> {
        let languages = Arc::new(LanguageBook::open(languages)?);
        let state = Arc::new(Mutex::new(State::default()));
        let config = Arc::new(Mutex::new(config));
        let notify = Arc::new(tokio::sync::Notify::new());
        let (s, c, n) = (state.clone(), config.clone(), notify.clone());
        let worker = tokio::spawn(async move {
            let mut running = tokio::task::JoinSet::new();
            loop {
                while running.len() < CONCURRENCY {
                    let job = s.lock().unwrap_or_else(|e| e.into_inner()).pop();
                    let Some(job) = job else { break };
                    if Instant::now() >= job.deadline {
                        job.miss();
                        continue;
                    }
                    running.spawn(run_job(job, translator.clone(), s.clone(), c.clone()));
                }
                tokio::select! {
                    _ = n.notified() => {},
                    _ = running.join_next(), if !running.is_empty() => {},
                    // Expire queued low-priority jobs even while all workers are occupied.
                    _ = tokio::time::sleep(Duration::from_millis(5)) => {
                        let mut state = s.lock().unwrap_or_else(|e| e.into_inner());
                        let mut i=0;
                        while i < state.queue.len() {
                            if Instant::now() >= state.queue[i].deadline { state.queue.remove(i).miss(); }
                            else { i+=1; }
                        }
                    }
                }
            }
        });
        Ok(Self { state, config, notify, worker, languages })
    }
    pub fn configure(&self, config: TranslationConfig) {
        *self.config.lock().unwrap_or_else(|e| e.into_inner()) = config;
    }
}
impl TranslationHook for TranslationPipeline {
    fn request(&self, value: Value, reply: TranslationReply) {
        if value["kind"] == "language" {
            self.languages.request(value, reply);
            return;
        }
        let fallback = json!({"req":value["req"],"slot":value["slot"],"miss":true});
        let Ok(request) = serde_json::from_value::<Request>(value) else {
            reply(fallback);
            return;
        };
        let now = Instant::now();
        let config = self.config.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if !config.enabled
            || !config.channels.contains(&request.channel)
            || config.command(&request.text)
            || request.req.len() > 128
            || request.slot >= 128
            || request.text.len() > 4096
            || request.text.chars().count() > 1024
            || request.player.is_empty()
            || request.player.len() > 128
            || !LANGUAGES.contains(&request.from.as_str())
            || request.to.len() > LANGUAGES.len()
            || request.to.keys().any(|l| !LANGUAGES.contains(&l.as_str()))
            || segments(&request.text).is_none()
        {
            reply(fallback);
            return;
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.queue.len() >= QUEUE
            || !state.admit(&request.player, config.per_player_per_minute, now)
        {
            drop(state);
            reply(fallback);
            return;
        }
        state.queue.push(Job { request, deadline: now + DEADLINE, reply });
        drop(state);
        self.notify.notify_one();
    }
}
async fn run_job(
    job: Job,
    translator: Arc<dyn Translator>,
    state: Arc<Mutex<State>>,
    config: Arc<Mutex<TranslationConfig>>,
) {
    let r = &job.request;
    let active = config.clone();
    let config = config.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if !config.enabled || !config.channels.contains(&r.channel) {
        job.miss();
        return;
    }
    let deadline = tokio::time::Instant::from_std(job.deadline);
    let translations = join_all(r.to.keys().filter(|to| **to != r.from).map(|to| async {
        let key = (r.text.clone(), r.from.clone(), to.clone());
        if let Some(v) = state.lock().unwrap_or_else(|e| e.into_inner()).cached(&key) {
            return safe(&config, &r.text, &v).then_some((to.clone(), v));
        }
        let work = async {
            let mut out = String::new();
            for (markup, text) in segments(&r.text)? {
                if markup || text.trim().is_empty() {
                    out.push_str(text);
                } else {
                    let translated = translator.translate(text, &r.from, to).await.ok()?;
                    if !safe(&config, text, &translated) {
                        return None;
                    }
                    out.push_str(&translated);
                }
            }
            safe(&config, &r.text, &out).then_some(out)
        };
        let out = tokio::time::timeout_at(deadline, work).await.ok().flatten()?;
        state.lock().unwrap_or_else(|e| e.into_inner()).cache(key, out.clone());
        Some((to.clone(), out))
    }))
    .await;
    let mut to: BTreeMap<_, _> = translations.into_iter().flatten().collect();
    let current = active.lock().unwrap_or_else(|e| e.into_inner()).clone();
    to.retain(|_, output| safe(&current, &r.text, output));
    if !current.enabled || !current.channels.contains(&r.channel) {
        job.miss();
        return;
    }
    // NWScript bounds the whole ASCII-escaped reply, not just each target text.
    loop {
        if to.is_empty() {
            job.miss();
            return;
        }
        let value = json!({"req":r.req,"slot":r.slot,"to":to});
        let bytes: usize = value
            .to_string()
            .chars()
            .map(|c| if c.is_ascii() { 1 } else { 6 * c.len_utf16() })
            .sum();
        if bytes <= 16_384 {
            (job.reply)(value);
            return;
        }
        to.pop_last();
    }
}

#[derive(Debug)]
struct LanguageBook {
    path: PathBuf,
    values: Mutex<BTreeMap<String, String>>,
    revisions: Mutex<BTreeMap<String, u64>>,
    next_revision: std::sync::atomic::AtomicU64,
    slots: Arc<tokio::sync::Semaphore>,
    runtime: tokio::runtime::Handle,
}
impl LanguageBook {
    fn open(path: PathBuf) -> Result<Self, String> {
        let values = if path.exists() {
            if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 1024 * 1024 {
                return Err("Language book too large".into());
            }
            serde_json::from_slice::<BTreeMap<String, String>>(
                &std::fs::read(&path).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?
        } else {
            BTreeMap::new()
        };
        if values.len() > 4096
            || values.iter().any(|(k, v)| !Self::key(k) || !LANGUAGES.contains(&v.as_str()))
        {
            return Err("Invalid language book".into());
        }
        Ok(Self {
            path,
            values: Mutex::new(values),
            revisions: Mutex::new(BTreeMap::new()),
            next_revision: std::sync::atomic::AtomicU64::new(1),
            slots: Arc::new(tokio::sync::Semaphore::new(16)),
            runtime: tokio::runtime::Handle::current(),
        })
    }
    fn key(key: &str) -> bool {
        !key.is_empty() && key.len() <= 128 && key.bytes().all(|b| b.is_ascii_alphanumeric())
    }
    fn save(&self, key: &str, lang: &str, revision: u64) -> Result<bool, std::io::Error> {
        let mut values = self.values.lock().unwrap_or_else(|e| e.into_inner());
        let mut revisions = self.revisions.lock().unwrap_or_else(|e| e.into_inner());
        if revisions.get(key).is_some_and(|last| *last >= revision) {
            return Ok(false);
        }
        if !Self::key(key)
            || !LANGUAGES.contains(&lang)
            || (!values.contains_key(key) && values.len() >= 4096)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid language preference",
            ));
        }
        let mut next = values.clone();
        next.insert(key.into(), lang.into());
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = self.path.with_extension("pending");
        let mut file = std::fs::File::create(&temp)?;
        use std::io::Write;
        file.write_all(&serde_json::to_vec(&next)?)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(temp, &self.path)?;
        *values = next;
        revisions.insert(key.into(), revision);
        Ok(true)
    }

    fn request(self: &Arc<Self>, value: Value, reply: TranslationReply) {
        let Ok(permit) = self.slots.clone().try_acquire_owned() else { return };
        // Assign before entering the blocking pool: worker lock order is not arrival order.
        use std::sync::atomic::Ordering;
        let Ok(revision) =
            self.next_revision
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        else {
            return;
        };
        let this = self.clone();
        self.runtime.spawn_blocking(move || {
            let _permit = permit;
            let Some(key) = value["player"].as_str().filter(|k| Self::key(k)) else { return };
            if value["req"].as_str().is_none_or(|s| s.len() > 128) || value["pc"].as_str().is_none_or(|s| s.len() > 16) { return; }
            if let Some(lang) = value["lang"].as_str()
                && !matches!(this.save(key, lang, revision), Ok(true)) { return; }
            let values = this.values.lock().unwrap_or_else(|e| e.into_inner());
            reply(json!({"kind":"language","req":value["req"],"pc":value["pc"],"lang":values.get(key).map(String::as_str).unwrap_or("en")}));
        });
    }
}

#[cfg(test)]
#[path = "translation_tests.rs"]
mod tests;
