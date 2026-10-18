use crate::{App, Principal};
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use mg_live::{
    gateway::Push,
    gateway_frames::{self as frames, Bye, ClientFrame, Connection, ServerFrame},
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::Notify,
    time::{Instant, timeout},
};

#[derive(Debug)]
struct Entry {
    text: String,
    area: Option<(String, String)>,
}
#[derive(Debug)]
struct Queue {
    entries: Mutex<VecDeque<Entry>>,
    notify: Notify,
    frames: usize,
    bytes: usize,
}
impl Queue {
    fn new(app: &App) -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
            frames: app.config.queue_frames,
            bytes: app.config.queue_bytes,
        }
    }
    fn put(&self, frame: ServerFrame) -> Result<(), &'static str> {
        let area = match &frame {
            ServerFrame::Push(Push::Area(a)) => Some((a.boot.clone(), a.area.clone())),
            _ => None,
        };
        let text = serde_json::to_string(&frame).map_err(|_| "invalid")?;
        if text.len() > frames::SERVER_FRAME_LIMIT {
            return Err("too_large");
        }
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(key) = &area {
            entries.retain(|e| e.area.as_ref() != Some(key));
        }
        if entries.len() >= self.frames
            || entries.iter().map(|e| e.text.len()).sum::<usize>().saturating_add(text.len())
                > self.bytes
        {
            return Err("overloaded");
        }
        entries.push_back(Entry { text, area });
        self.notify.notify_one();
        Ok(())
    }
    fn latest(&self, mut entry: Entry) -> Entry {
        if let Some(key) = &entry.area {
            let mut queue = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(index) = queue.iter().position(|e| e.area.as_ref() == Some(key)) {
                entry = queue.remove(index).expect("known entry");
            }
        }
        entry
    }
    async fn next(&self) -> Entry {
        loop {
            let notified = self.notify.notified();
            if let Some(e) = self.entries.lock().unwrap_or_else(|e| e.into_inner()).pop_front() {
                return e;
            }
            notified.await;
        }
    }
}
async fn revalidate(app: &App, token: &str, principal: &Principal) -> Result<(), &'static str> {
    let current = crate::authenticate_token(app, token).await;
    match current {
        None => Err("revoked"),
        Some(p) if &p != principal => Err("role_changed"),
        _ => Ok(()),
    }
}
pub(crate) async fn run(mut socket: WebSocket, app: App, token: String, principal: Principal) {
    let mut connection = Connection::default();
    let first = timeout(Duration::from_secs(45), socket.next()).await;
    let hello = match first {
        Ok(Some(Ok(Message::Text(text)))) => match frames::decode_client(text.as_bytes()) {
            Ok(frame) => match connection.receive(&frame) {
                Ok(()) => match frame {
                    ClientFrame::Hello(h) => h,
                    _ => {
                        close(&mut socket, "invalid").await;
                        return;
                    }
                },
                Err(e) => {
                    close(&mut socket, &e.code).await;
                    return;
                }
            },
            Err(e) => {
                close(&mut socket, &e.code).await;
                return;
            }
        },
        _ => {
            close(&mut socket, "invalid").await;
            return;
        }
    };
    if let Err(code) = revalidate(&app, &token, &principal).await {
        close(&mut socket, code).await;
        return;
    }
    let policy = app.policy.clone();
    let Ok(()) = app.core.with_store(move |s| s.set_policy(policy.policy())).await else {
        return;
    };
    let (generation, welcome) =
        match app.core.connect(principal.clone(), hello, app.config.clone()).await {
            Ok(v) => v,
            Err(_) => {
                close(&mut socket, "invalid").await;
                return;
            }
        };
    let text = match serde_json::to_string(&ServerFrame::Welcome(welcome)) {
        Ok(text) if text.len() <= frames::SERVER_FRAME_LIMIT => text,
        _ => {
            close(&mut socket, "too_large").await;
            return;
        }
    };
    if !matches!(
        timeout(Duration::from_secs(5), socket.send(Message::Text(text.into()))).await,
        Ok(Ok(()))
    ) {
        return;
    }
    let queue = Arc::new(Queue::new(&app));
    let (mut sink, mut stream) = socket.split();
    let (pings, mut ping_rx) = tokio::sync::mpsc::channel(2);
    let last_seen = Arc::new(Mutex::new(Instant::now()));
    let reader = async {
        let mut requests = 0u32;
        while let Some(message) = stream.next().await {
            *last_seen.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
            match message {
                Ok(Message::Text(text)) => {
                    let frame = frames::decode_client(text.as_bytes()).map_err(|e| e.code)?;
                    connection.receive(&frame).map_err(|e| e.code)?;
                    revalidate(&app, &token, &principal).await.map_err(str::to_owned)?;
                    let id = principal.id.clone();
                    match frame {
                        ClientFrame::Req(req) => {
                            requests += 1;
                            if requests > 100_000 {
                                return Err("overloaded".into());
                            }
                            let request_id = req.id;
                            let response = app
                                .core
                                .session(id, generation, move |g, id| {
                                    Ok(g.handle(id, req, crate::now()))
                                })
                                .await?;
                            queue.put(ServerFrame::Res(response)).map_err(str::to_owned)?;
                            connection.complete(request_id);
                        }
                        ClientFrame::Ack(ack) => {
                            app.core
                                .session(id, generation, move |g, id| {
                                    g.ack(id, &ack.results, ack.event_n)
                                })
                                .await?
                        }
                        ClientFrame::Hello(_) => return Err("invalid".into()),
                    }
                }
                Ok(Message::Ping(bytes)) => {
                    pings.try_send(Message::Pong(bytes)).map_err(|_| "overloaded".to_owned())?;
                }
                Ok(Message::Pong(_)) => {}
                Ok(Message::Close(_)) => return Ok(()),
                Ok(Message::Binary(_)) => return Err("invalid".into()),
                Err(_) => return Err("too_large".into()),
            }
        }
        Ok::<(), String>(())
    };
    let pump = async {
        let mut tick = tokio::time::interval(Duration::from_millis(50));
        let mut ping = Instant::now();
        loop {
            tick.tick().await;
            if app.core.is_shutdown() {
                return Err("shutdown".into());
            }
            if last_seen.lock().unwrap_or_else(|e| e.into_inner()).elapsed()
                >= Duration::from_secs(45)
            {
                return Err("idle".to_owned());
            }
            if ping.elapsed() >= Duration::from_secs(15) {
                revalidate(&app, &token, &principal).await.map_err(str::to_owned)?;
                let policy = app.policy.clone();
                app.core.with_store(move |s| s.set_policy(policy.policy())).await?;
                pings
                    .try_send(Message::Ping(Vec::new().into()))
                    .map_err(|_| "overloaded".to_owned())?;
                ping = Instant::now();
            }
            let pushes = app
                .core
                .session(principal.id.clone(), generation, |g, id| g.pushes(id, crate::now()))
                .await?;
            for push in pushes {
                queue.put(ServerFrame::Push(push)).map_err(str::to_owned)?;
            }
        }
    };
    let writer = async {
        let mut sent = std::collections::HashMap::<(String, String), Instant>::new();
        loop {
            let (message, key) = tokio::select! {
                entry = queue.next() => {
                    if let Some(last)=entry.area.as_ref().and_then(|key|sent.get(key)) {
                        tokio::time::sleep_until(*last+Duration::from_millis(250)).await;
                    }
                    let entry=queue.latest(entry);
                    (Message::Text(entry.text.into()),entry.area)
                },
                Some(message) = ping_rx.recv() => (message,None)
            };
            timeout(Duration::from_secs(5), sink.send(message))
                .await
                .map_err(|_| "overloaded".to_owned())?
                .map_err(|_| "closed".to_owned())?;
            if let Some(key) = key {
                sent.retain(|_, at| at.elapsed() < Duration::from_secs(1));
                sent.insert(key, Instant::now());
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), String>(())
    };
    let result = tokio::select! { r = reader => r, r = pump => r, r = writer => r };
    if let Err(code) = result {
        close(&mut sink, &code).await;
    }
    let _ =
        app.core.session(principal.id, generation, |g, id| g.disconnect(id, crate::now())).await;
}
async fn close<S: futures_util::Sink<Message> + Unpin>(socket: &mut S, code: &str) {
    let normal = matches!(
        code,
        "revoked" | "role_changed" | "overloaded" | "shutdown" | "upgrade" | "replaced"
    );
    if normal {
        let text =
            serde_json::to_string(&ServerFrame::Bye(Bye { code: code.into(), msg: code.into() }))
                .unwrap_or_default();
        let _ = timeout(Duration::from_secs(1), socket.send(Message::Text(text.into()))).await;
    }
    let (status, reason) = match code {
        "too_large" => (1009, "message too big"),
        "idle" => (1001, "idle"),
        "overloaded" => (1013, "overloaded"),
        "shutdown" => (1001, "shutdown"),
        "replaced" => (1000, "replaced"),
        "revoked" | "role_changed" => (1008, "authorization changed"),
        _ => (1002, "invalid protocol"),
    };
    let _ = timeout(
        Duration::from_secs(1),
        socket.send(Message::Close(Some(CloseFrame { code: status, reason: reason.into() }))),
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn queue_keeps_only_newest_area() {
        let q = Queue {
            entries: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
            frames: 1,
            bytes: 16384,
        };
        let store = mg_live::store::LiveStore::default();
        store.sync(br#"{"v":1,"boot":"b","areas":[{"area":"a"}]}"#, 1).unwrap();
        let live = store.area("a").unwrap();
        for rev in [1, 2, 3] {
            q.put(ServerFrame::Push(Push::Area(mg_live::gateway::Area {
                boot: "b".into(),
                area: "a".into(),
                rev,
                age_ms: 0,
                live: live.clone(),
            })))
            .unwrap();
        }
        let value: serde_json::Value = serde_json::from_str(&q.next().await.text).unwrap();
        assert_eq!(value["body"]["rev"], 3);
    }
    #[test]
    fn queue_rejects_overflow() {
        let q = Queue {
            entries: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
            frames: 1,
            bytes: 1024,
        };
        let frame = || ServerFrame::Push(Push::Notice { level: "info".into(), text: "one".into() });
        q.put(frame()).unwrap();
        assert_eq!(q.put(frame()), Err("overloaded"));
    }
}
