//! Independent Redis connections: reports, commands, and durable events.
#[path = "event_stream.rs"]
pub(crate) mod event_stream;
use crate::{
    now_ms,
    resp::{self, Reply},
    store::LiveStore,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Fast, nonblocking hand-off from Redis pub/sub. Implementations must bound their
/// queue and invoke the reply at most once; the game owns the fallback deadline.
pub trait TranslationHook: std::fmt::Debug + Send + Sync {
    fn request(&self, request: Value, reply: TranslationReply);
}
pub type TranslationReply = Box<dyn FnOnce(Value) + Send>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedisConfig {
    pub host: String,
    pub port: u16,
    pub prefix: String,
}

impl RedisConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.host.trim().is_empty() || self.port == 0 {
            return Err("Redis needs a host and nonzero port".into());
        }
        if self.prefix.is_empty()
            || self.prefix.len() > 64
            || !self.prefix.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-:.".contains(&b))
        {
            return Err("Redis prefix needs 1-64 letters, digits, _, -, : or .".into());
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub(crate) struct State {
    subscriber: bool,
    publisher: bool,
    last_state_ms: Option<u64>,
    last_error: String,
    module_status: Option<String>,
    published: u64,
    received: u64,
    generation: u64,
    reported: Option<(u64, String)>,
    events: event_stream::SharedStatus,
}

pub(crate) fn status(state: &Mutex<State>) -> Value {
    let s = lock(state);
    json!({"connected":s.subscriber && s.publisher,"moduleStatus":s.module_status,"lastStateMs":s.last_state_ms,"lastError":s.last_error,"published":s.published,"received":s.received,"events":*lock(&s.events)})
}

/// The one outbound encoding boundary: NWScript JsonParse consumes the server's
/// local codepage. ASCII JSON escapes carry Unicode through it without UTF-8
/// bytes being mistaken for local characters. Keep API/store values as Unicode.
pub(crate) fn game_json(value: &Value) -> String {
    use std::fmt::Write;
    let json = value.to_string();
    let mut escaped = String::with_capacity(json.len());
    for ch in json.chars() {
        if ch.is_ascii() {
            escaped.push(ch);
        } else {
            for unit in ch.encode_utf16(&mut [0; 2]) {
                write!(&mut escaped, "\\u{unit:04x}").expect("writing into a String");
            }
        }
    }
    escaped
}

#[derive(Deserialize)]
struct ReportIdentity {
    #[serde(default)]
    prefix: String,
    #[serde(default)]
    script_version: u32,
}

const SCRIPT_VERSION: u32 = 2;
const REBUILD_MODULE: &str =
    "this module was prepared by an older Moonglow; rebuild it with mg live-build";

fn check_module(body: &[u8], prefix: &str) -> Result<(), String> {
    let report: ReportIdentity = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    if report.script_version < SCRIPT_VERSION || report.prefix.is_empty() {
        return Err(REBUILD_MODULE.into());
    }
    if report.script_version != SCRIPT_VERSION {
        return Err("This module requires a newer Moonglow; update the application".into());
    }
    if report.prefix != prefix {
        return Err("State server prefix mismatch".into());
    }
    Ok(())
}

fn has_outbound(value: &Value) -> bool {
    ["commands", "received"].iter().any(|key| value[key].as_array().is_some_and(|a| !a.is_empty()))
}

/// Lifetime is owned by LiveService; its shared stop flag cancels both workers.
#[derive(Debug)]
pub struct Bridge {
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    kick: Sender<()>,
    threads: Vec<JoinHandle<()>>,
    inbox: event_stream::SharedInbox,
}

fn lock<T>(state: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

fn connection_state(state: &Mutex<State>, subscriber: bool, result: Result<(), String>) {
    let mut s = lock(state);
    s.generation += 1;
    s.reported = None;
    if subscriber {
        s.subscriber = result.is_ok();
    } else {
        s.publisher = result.is_ok();
    }
    if let Err(e) = result {
        s.last_error = e;
    } else if s.subscriber && s.publisher {
        s.last_error.clear();
    }
}

pub(crate) fn pause(stop: &AtomicBool, delay: Duration) {
    let until = Instant::now() + delay;
    while !stop.load(Ordering::Acquire) && Instant::now() < until {
        thread::sleep(
            Duration::from_millis(20).min(until.saturating_duration_since(Instant::now())),
        );
    }
}

pub(crate) fn connect(
    addresses: &[SocketAddr],
    stop: &AtomicBool,
) -> Result<BufReader<TcpStream>, String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut error = "Redis address is unavailable".to_owned();
    for address in addresses {
        if stop.load(Ordering::Acquire) {
            return Err("stopped".into());
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match TcpStream::connect_timeout(address, left.min(Duration::from_millis(500))) {
            Ok(stream) => {
                stream.set_read_timeout(Some(Duration::from_secs(1))).map_err(|e| e.to_string())?;
                stream
                    .set_write_timeout(Some(Duration::from_secs(1)))
                    .map_err(|e| e.to_string())?;
                return Ok(BufReader::new(stream));
            }
            Err(e) => error = format!("Redis connect: {e}"),
        }
    }
    Err(error)
}

pub(crate) fn request(
    connection: &mut BufReader<TcpStream>,
    args: &[&[u8]],
) -> Result<Reply, String> {
    connection.get_mut().write_all(&resp::encode(args)).map_err(|e| format!("Redis write: {e}"))?;
    resp::read_reply(connection)
}

impl Bridge {
    /// Headless owner: initialize durable command identities before workers can
    /// accept commands, then attach the existing durable event inbox.
    pub fn start_persistent(
        config: RedisConfig,
        store: Arc<LiveStore>,
        wake: Arc<dyn Fn() + Send + Sync>,
        stop: Arc<AtomicBool>,
        data: &std::path::Path,
    ) -> Result<Self, String> {
        config.validate()?;
        let host = if config.host.eq_ignore_ascii_case("localhost")
            || config.host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
        {
            "loopback".to_owned()
        } else {
            config.host.to_ascii_lowercase()
        };
        let server = serde_json::to_string(&(host, config.port, &config.prefix))
            .map_err(|e| e.to_string())?;
        store.bind_server(&config);
        store.require_command_identity();
        store.set_command_identity(data, &server)?;
        let bridge = Self::start(config, store.clone(), wake, stop)?;
        bridge.event_storage(data, &server, &store)?;
        Ok(bridge)
    }
    pub fn start(
        config: RedisConfig,
        store: Arc<LiveStore>,
        wake: Arc<dyn Fn() + Send + Sync>,
        stop: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        config.validate()?;
        // Resolve before starting workers; shutdown never waits for DNS.
        let addresses: Vec<_> = (config.host.as_str(), config.port)
            .to_socket_addrs()
            .map_err(|e| format!("Redis host: {e}"))?
            .collect();
        if addresses.is_empty() {
            return Err("Redis host resolved to no address".into());
        }
        let state = Arc::new(Mutex::new(State::default()));
        store.bind_server(&config);
        store.set_redis_status(state.clone());
        let (kick, receive) = mpsc::channel();
        store.set_kick(kick.clone());
        let mut threads = Vec::new();
        let (xl_send, xl_receive) = mpsc::sync_channel::<Value>(128);
        {
            let (addresses, stop) = (addresses.clone(), stop.clone());
            let channel = format!("{}.xl", config.prefix).into_bytes();
            threads.push(thread::spawn(move || {
                let mut connection = None;
                while !stop.load(Ordering::Acquire) {
                    let Ok(reply) = xl_receive.recv_timeout(Duration::from_millis(50)) else {
                        continue;
                    };
                    if connection.is_none() {
                        connection = connect(&addresses, &stop).ok();
                    }
                    if let Some(c) = &mut connection {
                        let body = game_json(&reply);
                        if request(c, &[b"PUBLISH", &channel, body.as_bytes()]).is_err() {
                            connection = None;
                        }
                    }
                    // Never retry a stale translation. Game-side fallback remains authoritative.
                }
            }));
        }
        let inbox = Arc::new(Mutex::new(None));
        {
            let (config, addresses, inbox, status, store, stop, wake) = (
                config.clone(),
                addresses.clone(),
                inbox.clone(),
                lock(&state).events.clone(),
                store.clone(),
                stop.clone(),
                wake.clone(),
            );
            threads.push(thread::spawn(move || {
                event_stream::run(config, addresses, inbox, status, store, stop, wake)
            }));
        }
        {
            let (state, stop, store, wake, kick, addresses) = (
                state.clone(),
                stop.clone(),
                store.clone(),
                wake.clone(),
                kick.clone(),
                addresses.clone(),
            );
            let prefix = config.prefix.clone();
            let channel = format!("{}.state", config.prefix).into_bytes();
            // The server log (`tools/live/wsl/log-tail.sh`), into the live events.
            let log_channel = format!("{}.log", config.prefix).into_bytes();
            let event_channel = format!("{}.event", config.prefix).into_bytes();
            let xl_channel = format!("{}.xl.req", config.prefix).into_bytes();
            threads.push(thread::spawn(move || {
                let mut retry = 1;
                while !stop.load(Ordering::Acquire) {
                    let result = (|| {
                        let mut connection = connect(&addresses, &stop)?;
                        let confirmation = request(&mut connection, &[b"SUBSCRIBE", &channel])?;
                        if confirmation
                            != Reply::Array(Some(vec![
                                Reply::Bulk(Some(b"subscribe".to_vec())),
                                Reply::Bulk(Some(channel.clone())),
                                Reply::Int(1),
                            ]))
                        {
                            return Err("Redis refused subscription".into());
                        }
                        match request(&mut connection, &[b"SUBSCRIBE", &log_channel])? {
                            Reply::Array(Some(parts))
                                if parts.get(1)
                                    == Some(&Reply::Bulk(Some(log_channel.clone()))) => {}
                            _ => return Err("Redis refused the log subscription".into()),
                        }
                        match request(&mut connection, &[b"SUBSCRIBE", &event_channel])? {
                            Reply::Array(Some(parts))
                                if parts.get(1)
                                    == Some(&Reply::Bulk(Some(event_channel.clone()))) => {}
                            _ => return Err("Redis refused the event subscription".into()),
                        }
                        match request(&mut connection, &[b"SUBSCRIBE", &xl_channel])? {
                            Reply::Array(Some(parts))
                                if parts.get(1) == Some(&Reply::Bulk(Some(xl_channel.clone()))) => {
                            }
                            _ => return Err("Redis refused translation subscription".into()),
                        }
                        retry = 1;
                        connection_state(&state, true, Ok(()));
                        store.resend_watched_areas(now_ms());
                        wake();
                        while !stop.load(Ordering::Acquire) {
                            // A timeout at a frame boundary is normal; a partial-frame
                            // timeout below discards the connection, never the prefix.
                            match connection.fill_buf() {
                                Err(e)
                                    if matches!(
                                        e.kind(),
                                        std::io::ErrorKind::TimedOut
                                            | std::io::ErrorKind::WouldBlock
                                    ) =>
                                {
                                    continue;
                                }
                                Err(e) => return Err(format!("Redis subscribe: {e}")),
                                Ok([]) => return Err("Redis subscription closed".into()),
                                Ok(_) => {}
                            }
                            let Reply::Array(Some(parts)) = resp::read_reply(&mut connection)?
                            else {
                                return Err("Redis malformed pub/sub reply".into());
                            };
                            match parts.as_slice() {
                                [
                                    Reply::Bulk(Some(kind)),
                                    Reply::Bulk(Some(name)),
                                    Reply::Bulk(Some(body)),
                                ] if kind == b"message" && name == &channel => {
                                    let generation = lock(&state).generation;
                                    if let Err(message) = check_module(body, &prefix) {
                                        let mut s = lock(&state);
                                        s.last_error = message.clone();
                                        s.module_status = Some(message);
                                        s.reported = None;
                                        drop(s);
                                        wake();
                                        continue;
                                    }
                                    match store.sync(body, now_ms()) {
                                        Ok(reply) => {
                                            let mut s = lock(&state);
                                            s.last_state_ms = Some(now_ms());
                                            s.module_status = None;
                                            s.received += 1;
                                            if s.generation == generation
                                                && s.publisher
                                                && s.subscriber
                                            {
                                                s.reported = reply["boot"]
                                                    .as_str()
                                                    .map(|boot| (generation, boot.to_owned()));
                                            }
                                            drop(s);
                                            // A command can be enqueued between sync returning
                                            // and opening this generation's publish gate.
                                            let _ = kick.send(());
                                            wake();
                                        }
                                        Err(e) => {
                                            lock(&state).last_error = format!("Invalid state: {e}");
                                            wake();
                                        }
                                    }
                                }
                                [
                                    Reply::Bulk(Some(kind)),
                                    Reply::Bulk(Some(name)),
                                    Reply::Bulk(Some(body)),
                                ] if kind == b"message" && name == &log_channel => {
                                    if let Ok(message) = serde_json::from_slice(body) {
                                        store.server_log(&message, now_ms());
                                        wake();
                                    }
                                }
                                [
                                    Reply::Bulk(Some(kind)),
                                    Reply::Bulk(Some(name)),
                                    Reply::Bulk(Some(body)),
                                ] if kind == b"message" && name == &event_channel => {
                                    match store.event_packet(body, now_ms()) {
                                        Ok(true) => wake(),
                                        Ok(false) => {}
                                        Err(e) => {
                                            lock(&state).last_error = format!("Invalid event: {e}");
                                            wake();
                                        }
                                    }
                                }
                                [
                                    Reply::Bulk(Some(kind)),
                                    Reply::Bulk(Some(name)),
                                    Reply::Bulk(Some(body)),
                                ] if kind == b"message" && name == &xl_channel => {
                                    if body.len() <= 16_384
                                        && let Ok(mut value) = serde_json::from_slice::<Value>(body)
                                    {
                                        if value.get("req").and_then(Value::as_str).is_none() {
                                            continue;
                                        }
                                        crate::wire_text::translation(&mut value);
                                        let send = xl_send.clone();
                                        store.translate(
                                            value,
                                            Box::new(move |reply| {
                                                let _ = send.try_send(reply);
                                            }),
                                        );
                                    }
                                }
                                _ => return Err("Redis unexpected pub/sub reply".into()),
                            }
                        }
                        Ok(())
                    })();
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    connection_state(&state, true, result);
                    wake();
                    pause(&stop, Duration::from_secs(retry));
                    retry = (retry * 2).min(10);
                }
                lock(&state).subscriber = false;
            }));
        }
        {
            let (state, stop, wake) = (state.clone(), stop.clone(), wake.clone());
            let channel = format!("{}.cmd", config.prefix).into_bytes();
            threads.push(thread::spawn(move || {
                let mut retry = 1;
                while !stop.load(Ordering::Acquire) {
                    let result = (|| {
                        let mut connection = connect(&addresses, &stop)?;
                        if request(&mut connection, &[b"PING"])? != Reply::Simple("PONG".into()) { return Err("Redis PING failed".into()); }
                        retry = 1;
                        connection_state(&state, false, Ok(()));
                        store.resend_watched_areas(now_ms());
                        wake();
                        let mut sent = Instant::now();
                        let mut pending = true;
                        while !stop.load(Ordering::Acquire) {
                            if pending {
                                let reported = lock(&state).reported.clone();
                                let commands = store.peek_commands(now_ms());
                                let mut current = lock(&state);
                                if reported.as_ref().is_some_and(|(_, boot)| commands["boot"] == *boot)
                                    && current.reported == reported && current.subscriber && current.publisher
                                    && commands["prefix"] == config.prefix && !stop.load(Ordering::Acquire)
                                    && has_outbound(&commands) {
                                    let boot = reported.as_ref().expect("gate checked").1.clone();
                                    let published = store.handoff_commands(now_ms(), &boot, &config.prefix, |commands| {
                                        if !has_outbound(&commands) { return Ok::<bool, String>(false); }
                                        let body = game_json(&commands);
                                        if !matches!(request(&mut connection, &[b"PUBLISH", &channel, body.as_bytes()])?, Reply::Int(n) if n >= 0) { return Err("Redis PUBLISH failed".into()); }
                                        Ok(true)
                                    }).transpose()?.unwrap_or(false);
                                    current.published += u64::from(published);
                                    sent = Instant::now();
                                    drop(current);
                                    wake();
                                }
                            }
                            if sent.elapsed() >= Duration::from_secs(10) {
                                if request(&mut connection, &[b"PING"])? != Reply::Simple("PONG".into()) { return Err("Redis PING failed".into()); }
                                sent = Instant::now();
                            }
                            pending = receive.recv_timeout(Duration::from_millis(100)).is_ok();
                            for _ in 0..256 {
                                if receive.try_recv().is_err() {
                                    break;
                                }
                                pending = true;
                            }
                        }
                        Ok(())
                    })();
                    if stop.load(Ordering::Acquire) { break; }
                    connection_state(&state, false, result);
                    wake();
                    pause(&stop, Duration::from_secs(retry));
                    retry = (retry * 2).min(10);
                }
                lock(&state).publisher = false;
            }));
        }
        Ok(Self { state, stop, kick, threads, inbox })
    }

    pub(crate) fn event_storage(
        &self,
        data: &std::path::Path,
        server: &str,
        store: &LiveStore,
    ) -> Result<(), String> {
        let mut current = lock(&self.inbox);
        if current.is_none() {
            let inbox = event_stream::Inbox::new(data, server)?;
            inbox.restore(store)?;
            inbox.restored_status(&lock(&self.state).events);
            *current = Some(inbox);
        }
        Ok(())
    }

    pub fn status(&self) -> Value {
        status(&self.state)
    }

    pub fn join(&mut self) {
        let _ = self.kick.send(());
        for worker in self.threads.drain(..) {
            let _ = worker.join();
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_module_has_an_actionable_status_even_when_redis_is_connected() {
        for body in [json!({"v":1}), json!({"v":1,"prefix":"test"})] {
            let message = check_module(&serde_json::to_vec(&body).unwrap(), "test").unwrap_err();
            assert_eq!(message, REBUILD_MODULE);
            let state = Mutex::new(State {
                subscriber: true,
                publisher: true,
                module_status: Some(message),
                ..State::default()
            });
            assert_eq!(status(&state)["moduleStatus"], REBUILD_MODULE);
            assert_eq!(status(&state)["connected"], true);
        }
        assert!(check_module(br#"{"v":1,"prefix":"test","script_version":2}"#, "test").is_ok());
    }
    use std::net::{Shutdown, TcpListener};
    #[test]
    fn publisher_json_preserves_polish_and_json_escaping() {
        let text =
            "Za\u{017c}\u{00f3}\u{0142}\u{0107} g\u{0119}\u{015b}l\u{0105} ja\u{017a}\u{0144}";
        let value = json!({"commands":[{"op":"npc_say","text":text},
            {"op":"set_name","name":text}],"other":"\u{1f980}\n\"\\u017c"});
        let wire = game_json(&value);
        assert!(wire.is_ascii());
        assert!(wire.contains(r"Za\u017c\u00f3\u0142\u0107 g\u0119\u015bl\u0105 ja\u017a\u0144"));
        assert!(wire.contains(r"\ud83e\udd80"));
        assert_eq!(serde_json::from_str::<Value>(&wire).unwrap(), value);
        let ascii = json!({"code":"SpeakString(\"hi\");","text":"a\nb\t\\"});
        assert_eq!(game_json(&ascii), ascii.to_string());
    }

    type Subscriber = (Vec<u8>, TcpStream);

    struct Fake {
        address: SocketAddr,
        stop: Arc<AtomicBool>,
        subscribers: Arc<Mutex<Vec<Subscriber>>>,
        clients: Arc<Mutex<Vec<TcpStream>>>,
        published: mpsc::Receiver<(String, Value)>,
        worker: Option<JoinHandle<()>>,
    }

    impl Fake {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let subscribers = Arc::new(Mutex::new(Vec::new()));
            let clients = Arc::new(Mutex::new(Vec::new()));
            let (send, published) = mpsc::channel();
            let (stopped, subs, sockets) = (stop.clone(), subscribers.clone(), clients.clone());
            let worker = thread::spawn(move || {
                let mut workers = Vec::new();
                while !stopped.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            stream.set_nonblocking(false).unwrap();
                            stream.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
                            lock(&sockets).push(stream.try_clone().unwrap());
                            let (stopped, subs, send) =
                                (stopped.clone(), subs.clone(), send.clone());
                            workers.push(thread::spawn(move || {
                                let mut stream = BufReader::new(stream);
                                while !stopped.load(Ordering::Acquire) {
                                    match stream.fill_buf() {
                                        Err(e)
                                            if matches!(
                                                e.kind(),
                                                std::io::ErrorKind::WouldBlock
                                                    | std::io::ErrorKind::TimedOut
                                            ) =>
                                        {
                                            continue;
                                        }
                                        Err(_) | Ok([]) => break,
                                        Ok(_) => {}
                                    }
                                    let Ok(Reply::Array(Some(parts))) =
                                        resp::read_reply(&mut stream)
                                    else {
                                        break;
                                    };
                                    let args: Vec<Vec<u8>> =
                                        parts
                                            .into_iter()
                                            .map(|r| {
                                                if let Reply::Bulk(Some(b)) = r {
                                                    b
                                                } else {
                                                    vec![]
                                                }
                                            })
                                            .collect();
                                    match args.first().map(Vec::as_slice) {
                                        Some(b"PING") => {
                                            let _ = stream.get_mut().write_all(b"+PONG\r\n");
                                        }
                                        Some(b"SUBSCRIBE") if args.len() == 2 => {
                                            let channel = &args[1];
                                            let reply = format!(
                                                "*3\r\n$9\r\nsubscribe\r\n${}\r\n{}\r\n:1\r\n",
                                                channel.len(),
                                                String::from_utf8_lossy(channel)
                                            );
                                            stream.get_mut().write_all(reply.as_bytes()).unwrap();
                                            lock(&subs).push((
                                                channel.clone(),
                                                stream.get_ref().try_clone().unwrap(),
                                            ));
                                        }
                                        Some(b"PUBLISH") if args.len() == 3 => {
                                            let body = serde_json::from_slice(&args[2]).unwrap();
                                            let _ = send.send((
                                                String::from_utf8(args[1].clone()).unwrap(),
                                                body,
                                            ));
                                            let _ = stream.get_mut().write_all(b":1\r\n");
                                        }
                                        _ => {
                                            let _ = stream.get_mut().write_all(b"-ERR unknown\r\n");
                                        }
                                    }
                                }
                            }));
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(e) => panic!("fake accept: {e}"),
                    }
                }
                for stream in lock(&sockets).iter() {
                    let _ = stream.shutdown(Shutdown::Both);
                }
                for worker in workers {
                    let _ = worker.join();
                }
            });
            Self { address, stop, subscribers, clients, published, worker: Some(worker) }
        }
        fn state(&self, value: Value) {
            for (channel, stream) in lock(&self.subscribers).iter_mut() {
                let _ = stream.write_all(&resp::encode(&[
                    b"message",
                    channel,
                    value.to_string().as_bytes(),
                ]));
            }
        }
        fn publish(&self, to: &[u8], value: Value) {
            for (channel, stream) in lock(&self.subscribers).iter_mut().filter(|(c, _)| c == to) {
                let _ = stream.write_all(&resp::encode(&[
                    b"message",
                    channel,
                    value.to_string().as_bytes(),
                ]));
            }
        }
        fn disconnect(&self) {
            for stream in lock(&self.clients).iter() {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
    }
    impl Drop for Fake {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            self.disconnect();
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    fn wait(mut condition: impl FnMut() -> bool) {
        let until = Instant::now() + Duration::from_secs(6);
        while !condition() {
            assert!(Instant::now() < until, "condition timed out");
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn report(seq: u64) -> Value {
        json!({"v":1,"script_version":2,"prefix":"test","boot":"test","seq":seq,"areas":[{"area":"a","cre":[{"id":"ab"}]}]})
    }

    #[test]
    fn immediate_translation_lane_does_not_wait_for_state_reports() {
        #[derive(Debug)]
        struct Hook;
        impl TranslationHook for Hook {
            fn request(&self, value: Value, reply: TranslationReply) {
                assert_eq!(value["text"], "hello world");
                reply(json!({"req":value["req"], "slot":value["slot"], "to":{"pl":"witaj"}}));
            }
        }
        let fake = Fake::start();
        let store = Arc::new(LiveStore::default());
        store.set_translation_hook(Some(Arc::new(Hook)));
        let bridge = Bridge::start(
            RedisConfig {
                host: "127.0.0.1".into(),
                port: fake.address.port(),
                prefix: "test".into(),
            },
            store,
            Arc::new(|| {}),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        wait(|| lock(&fake.subscribers).iter().any(|(c, _)| c == b"test.xl.req"));
        fake.publish(
            b"test.xl.req",
            json!({"req":"boot:one","slot":1,"text":"damaged","text_bytes":"hello+world"}),
        );
        let (channel, reply) = fake.published.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(channel, "test.xl");
        assert_eq!(reply["to"]["pl"], "witaj");
        assert!(bridge.status()["lastStateMs"].is_null());
    }

    #[test]
    fn closed_publish_gate_keeps_commands_revocable() {
        let fake = Fake::start();
        let store = Arc::new(LiveStore::default());
        let config = RedisConfig {
            host: "127.0.0.1".into(),
            port: fake.address.port(),
            prefix: "test".into(),
        };
        store.bind_server(&config);
        store.sync(&serde_json::to_vec(&report(1)).unwrap(), now_ms()).unwrap();
        let command =
            serde_json::from_value(json!({"id":"revoked","area":"a","op":"destroy","target":"ab"}))
                .unwrap();
        store.enqueue(command, now_ms()).unwrap();
        // Reconnect to the same endpoint: retained state permits enqueue, but
        // publication must wait for the new subscriber's first report.
        let stop = Arc::new(AtomicBool::new(false));
        let mut bridge =
            Bridge::start(config, store.clone(), Arc::new(|| {}), stop.clone()).unwrap();
        wait(|| bridge.status()["connected"] == true && !lock(&fake.subscribers).is_empty());
        assert!(fake.published.recv_timeout(Duration::from_millis(250)).is_err());
        assert_eq!(store.revoke_owner("local", "revoked during reconnect"), 1);
        fake.state(report(2));
        wait(|| bridge.status()["lastStateMs"].is_number());
        assert!(fake.published.recv_timeout(Duration::from_millis(250)).is_err());
        stop.store(true, Ordering::Release);
        bridge.join();
    }

    #[test]
    fn endpoint_switch_waits_for_new_report_and_discards_old_queue() {
        let fake = Fake::start();
        let store = Arc::new(LiveStore::default());
        let mut config = RedisConfig { host: "127.0.0.1".into(), port: 1, prefix: "test".into() };
        store.bind_server(&config);
        store.sync(&serde_json::to_vec(&report(1)).unwrap(), now_ms()).unwrap();
        let command = |id: &str| {
            serde_json::from_value(json!({"id":id,"area":"a","op":"destroy","target":"ab"}))
                .unwrap()
        };
        store.enqueue(command("old"), now_ms()).unwrap();
        config.port = fake.address.port();
        let stop = Arc::new(AtomicBool::new(false));
        let mut bridge =
            Bridge::start(config, store.clone(), Arc::new(|| {}), stop.clone()).unwrap();
        wait(|| bridge.status()["connected"] == true && !lock(&fake.subscribers).is_empty());
        assert!(fake.published.recv_timeout(Duration::from_millis(250)).is_err());
        assert_eq!(store.result("old").unwrap().code, Some(crate::store::ErrorCode::Expired));
        assert!(store.enqueue(command("early"), now_ms()).is_err());
        let mut wrong = report(2);
        wrong["prefix"] = json!("another-server");
        fake.state(wrong);
        assert!(fake.published.recv_timeout(Duration::from_millis(250)).is_err());
        assert!(store.area("a").is_none());
        fake.state(report(3));
        wait(|| store.area("a").is_some());
        store.enqueue(command("fresh"), now_ms()).unwrap();
        let (_, wire) = fake.published.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(wire["boot"], "test");
        assert_eq!(wire["prefix"], "test");
        assert_eq!(wire["commands"][0]["id"], "fresh");
        stop.store(true, Ordering::Release);
        bridge.join();
    }

    #[test]
    fn endpoint_change_resets_history_and_identities_but_keeps_sent_outcome_unknown() {
        let store = LiveStore::default();
        let mut config = RedisConfig { host: "localhost".into(), port: 1, prefix: "one".into() };
        store.bind_server(&config);
        store.sync(&serde_json::to_vec(&report(1)).unwrap(), 0).unwrap();
        store.server_log(
            &json!({"kind":"server_log","stream":"old","seq":1,"line":"old endpoint"}),
            0,
        );
        let command = || {
            serde_json::from_value(json!({"id":"reuse","area":"a","op":"destroy","target":"ab"}))
                .unwrap()
        };
        store.enqueue(command(), 0).unwrap();
        store.commands(0);
        config.prefix = "two".into();
        store.bind_server(&config);
        assert_eq!(store.result("reuse").unwrap().code, Some(crate::store::ErrorCode::Unknown));
        assert!(
            store
                .events_scoped(&crate::events::Query::default(), &crate::access::Scope::default())
                .0
                .is_empty()
        );
        store.sync(&serde_json::to_vec(&report(1)).unwrap(), 1).unwrap();
        store.enqueue(command(), 1).unwrap();
        assert!(store.result("reuse").is_none());
        assert_eq!(store.commands(1)["commands"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn bridge_reports_kicks_retries_and_reconnects() {
        let fake = Fake::start();
        let store = Arc::new(LiveStore::default());
        let stop = Arc::new(AtomicBool::new(false));
        let mut bridge = Bridge::start(
            RedisConfig {
                host: "127.0.0.1".into(),
                port: fake.address.port(),
                prefix: "test".into(),
            },
            store.clone(),
            Arc::new(|| {}),
            stop.clone(),
        )
        .unwrap();
        wait(|| bridge.status()["connected"] == true && !lock(&fake.subscribers).is_empty());
        assert_eq!(store.status(now_ms())["redis"]["connected"], true);
        fake.state(report(1));
        wait(|| store.area("a").is_some());
        // The server log's channel feeds the live events.
        wait(|| lock(&fake.subscribers).iter().any(|(c, _)| c == b"test.log"));
        let line = json!({"v":1,"kind":"server_log","stream":"s","seq":1,"line":"Script error: x"});
        fake.publish(b"test.log", line);
        wait(|| {
            store.events(&crate::events::Query::default()).0.iter().any(|e| e.kind == "server_log")
        });
        assert!(fake.published.try_recv().is_err(), "empty queue must not publish");
        let command =
            serde_json::from_value(json!({"id":"once","area":"a","op":"destroy","target":"ab"}))
                .unwrap();
        store.enqueue(command, now_ms()).unwrap();
        let (channel, body) = fake.published.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(channel, "test.cmd");
        assert_eq!(body["commands"][0]["id"], "once");
        fake.state(report(2));
        assert_eq!(
            fake.published.recv_timeout(Duration::from_secs(2)).unwrap().1["commands"][0]["id"],
            "once"
        );
        fake.disconnect();
        // (Each connection also subscribes to the server log: count the state's.)
        wait(|| lock(&fake.subscribers).iter().filter(|(c, _)| c == b"test.state").count() >= 2);
        wait(|| bridge.status()["connected"] == true);
        assert!(
            fake.published.recv_timeout(Duration::from_millis(250)).is_err(),
            "reconnect must wait for a fresh game report"
        );
        fake.state(
            json!({"v":1,"script_version":2,"prefix":"test","boot":"test","seq":3,"ack":["once"]}),
        );
        wait(|| store.status(now_ms())["queued"].as_array().unwrap().is_empty());
        assert_eq!(
            store.status(now_ms())["results"][0]["result"]["err"],
            "unknown: acknowledged; result not received"
        );
        let before = Instant::now();
        stop.store(true, Ordering::Release);
        bridge.join();
        assert_eq!(store.status(now_ms())["redis"]["connected"], false);
        assert!(before.elapsed() < Duration::from_secs(2));
    }

    #[test]
    #[ignore = "requires a local Redis instance"]
    fn real_redis() {
        let prefix = format!("mgtest_{}_{}", std::process::id(), now_ms());
        let store = Arc::new(LiveStore::default());
        let stop = Arc::new(AtomicBool::new(false));
        let mut bridge = Bridge::start(
            RedisConfig { host: "127.0.0.1".into(), port: 6379, prefix: prefix.clone() },
            store.clone(),
            Arc::new(|| {}),
            stop.clone(),
        )
        .unwrap();
        wait(|| bridge.status()["connected"] == true);
        let mut client = connect(&["127.0.0.1:6379".parse().unwrap()], &stop).unwrap();
        let mut body = report(1);
        body["prefix"] = json!(prefix);
        let body = body.to_string();
        assert!(
            matches!(request(&mut client, &[b"PUBLISH", format!("{prefix}.state").as_bytes(), body.as_bytes()]).unwrap(), Reply::Int(n) if n >= 1)
        );
        wait(|| store.area("a").is_some());
        stop.store(true, Ordering::Release);
        bridge.join();
    }
}
