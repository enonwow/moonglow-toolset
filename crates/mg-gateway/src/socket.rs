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
    async fn next(&self) -> String {
        loop {
            let notified = self.notify.notified();
            if let Some(e) = self.entries.lock().unwrap_or_else(|e| e.into_inner()).pop_front() {
                return e.text;
            }
            notified.await;
        }
    }
}
async fn revalidate(app: &App, token: &str, principal: &Principal) -> Result<(), &'static str> {
    let auth = app.auth.clone();
    let token = token.to_owned();
    let current =
        tokio::task::spawn_blocking(move || auth.authenticate(&token)).await.ok().flatten();
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
    let queue = Arc::new(Queue::new(&app));
    if let Err(code) = queue.put(ServerFrame::Welcome(welcome)) {
        close(&mut socket, code).await;
        return;
    }
    let (mut sink, mut stream) = socket.split();
    let (pings, mut ping_rx) = tokio::sync::mpsc::channel(2);
    let last_seen = Arc::new(Mutex::new(Instant::now()));
    let reader = async {
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
        loop {
            let message = tokio::select! { text = queue.next() => Message::Text(text.into()), Some(message) = ping_rx.recv() => message };
            timeout(Duration::from_secs(5), sink.send(message))
                .await
                .map_err(|_| "overloaded".to_owned())?
                .map_err(|_| "closed".to_owned())?;
        }
        #[allow(unreachable_code)]
        Ok::<(), String>(())
    };
    let result = tokio::select! { r = reader => r, r = pump => r, r = writer => r };
    if let Err(code) = result {
        let bye =
            serde_json::to_string(&ServerFrame::Bye(Bye { code: code.clone(), msg: code.clone() }))
                .unwrap_or_default();
        let _ = timeout(Duration::from_secs(1), sink.send(Message::Text(bye.into()))).await;
        let _ = timeout(
            Duration::from_secs(1),
            sink.send(Message::Close(Some(CloseFrame {
                code: if code == "too_large" { 1009 } else { 1008 },
                reason: code.into(),
            }))),
        )
        .await;
    }
    let _ =
        app.core.session(principal.id, generation, |g, id| g.disconnect(id, crate::now())).await;
}
async fn close(socket: &mut WebSocket, code: &str) {
    let text =
        serde_json::to_string(&ServerFrame::Bye(Bye { code: code.into(), msg: code.into() }))
            .unwrap_or_default();
    let _ = timeout(Duration::from_secs(1), socket.send(Message::Text(text.into()))).await;
    let _ = timeout(
        Duration::from_secs(1),
        socket.send(Message::Close(Some(CloseFrame {
            code: if code == "too_large" { 1009 } else { 1008 },
            reason: code.to_owned().into(),
        }))),
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
        let value: serde_json::Value = serde_json::from_str(&q.next().await).unwrap();
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
