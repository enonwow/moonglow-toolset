use crate::{
    Gateway, LinkError, LinkSnapshot, LinkStatus,
    credentials::{Backend, TokenStore},
    state::Replica,
};
use mg_live::store::{Command, CommandError, LiveStore};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Debug, Clone)]
pub struct LinkConfig {
    pub server_id: String,
    pub gateway: Gateway,
    pub watch: Vec<String>,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
}
impl LinkConfig {
    pub fn new(server_id: impl Into<String>, gateway: Gateway) -> Self {
        Self {
            server_id: server_id.into(),
            gateway,
            watch: Vec::new(),
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error(transparent)]
    Trust(#[from] crate::Error),
    #[error(transparent)]
    Link(#[from] LinkError),
}

/// Poll from egui and use the wake callback for repaint; never wait on its UI thread.
#[derive(Debug)]
pub struct PendingCall(mpsc::Receiver<Result<Value, LinkError>>);
impl PendingCall {
    pub fn try_recv(&self) -> Result<Option<Value>, LinkError> {
        match self.0.try_recv() {
            Ok(result) => result.map(Some),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(LinkError::Stopped),
        }
    }
    pub fn wait(self, timeout: Duration) -> Result<Value, LinkError> {
        self.0.recv_timeout(timeout).map_err(|e| match e {
            mpsc::RecvTimeoutError::Timeout => LinkError::Timeout,
            mpsc::RecvTimeoutError::Disconnected => LinkError::Stopped,
        })?
    }
}

pub(crate) struct Outgoing {
    pub op: String,
    pub a: Value,
    pub reply: mpsc::Sender<Result<Value, LinkError>>,
    pub _permit: OwnedSemaphorePermit,
}
pub(crate) struct Shared {
    pub replica: Mutex<Replica>,
    pub store: Arc<LiveStore>,
    pub watch: Mutex<Vec<String>>,
    pub capacity: Arc<Semaphore>,
    pub wake: Arc<dyn Fn() + Send + Sync>,
}

/// Owns one server's mirror and worker. Dropping it stops the worker, without
/// touching global campaign context, another service or the credential store.
pub struct LinkService {
    config: LinkConfig,
    pub(crate) shared: Arc<Shared>,
    outgoing: tokio::sync::mpsc::Sender<Outgoing>,
    stop: tokio::sync::watch::Sender<bool>,
    worker: Option<JoinHandle<()>>,
}
impl std::fmt::Debug for LinkService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkService")
            .field("status", &self.snapshot().status)
            .finish_non_exhaustive()
    }
}
impl LinkService {
    /// Credential access and first-use IP trust discovery may block: invoke on
    /// a startup worker. TrustRequired includes the same prompt data as Login.
    pub fn start<B: Backend>(
        config: LinkConfig,
        tokens: &TokenStore<B>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, StartError> {
        if config.server_id.is_empty()
            || config.connect_timeout.is_zero()
            || config.request_timeout.is_zero()
            || config.connect_timeout > Duration::from_secs(60)
            || config.request_timeout > Duration::from_secs(300)
        {
            return Err(LinkError::Protocol.into());
        }
        validate_watch(&config.watch, 16)?;
        config.gateway.check_trust(config.connect_timeout)?;
        let token = tokens
            .get(&config.server_id)
            .map_err(|_| LinkError::Credentials)?
            .ok_or(LinkError::Credentials)?;
        // A bearer is an ASCII HTTP credential, not arbitrary Unicode header data.
        if !token.expose().bytes().all(|b| b.is_ascii_graphic()) {
            return Err(LinkError::Credentials.into());
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| LinkError::Transport)?;
        let shared = Arc::new(Shared {
            replica: Mutex::new(Replica::default()),
            store: Arc::new(LiveStore::new_mirror()),
            watch: Mutex::new(config.watch.clone()),
            capacity: Arc::new(Semaphore::new(64)),
            wake,
        });
        let (outgoing, receiver) = tokio::sync::mpsc::channel(64);
        let (stop, stopping) = tokio::sync::watch::channel(false);
        let worker_shared = shared.clone();
        let worker_config = config.clone();
        let worker = std::thread::Builder::new()
            .name("live-link".into())
            .spawn(move || {
                runtime.block_on(crate::worker::run(
                    worker_config,
                    token,
                    worker_shared,
                    receiver,
                    stopping,
                ));
            })
            .map_err(|_| LinkError::Transport)?;
        Ok(Self { config, shared, outgoing, stop, worker: Some(worker) })
    }
    pub fn config(&self) -> &LinkConfig {
        &self.config
    }
    pub fn store(&self) -> Arc<LiveStore> {
        self.shared.store.clone()
    }
    pub fn snapshot(&self) -> LinkSnapshot {
        self.shared.replica.lock().unwrap_or_else(|e| e.into_inner()).snapshot.clone()
    }
    pub fn status(&self) -> Value {
        let mut value = self.shared.store.status(mg_live::now_ms());
        value["gateway"] = self.connection_status();
        value
    }
    pub fn connection_status(&self) -> Value {
        let replica = self.shared.replica.lock().unwrap_or_else(|e| e.into_inner());
        json!({"connected":replica.snapshot.status==LinkStatus::Connected,
            "state":format!("{:?}",replica.snapshot.status),"error":replica.snapshot.error.map(|e|e.to_string())})
    }
    pub fn enqueue(&self, command: Command) -> Result<String, CommandError> {
        self.shared.store.enqueue(command, mg_live::now_ms())
    }
    pub fn request(&self, op: &str, a: Value) -> Result<PendingCall, LinkError> {
        let replica = self.shared.replica.lock().unwrap_or_else(|e| e.into_inner());
        if replica.snapshot.status != LinkStatus::Connected {
            return Err(LinkError::Transport);
        }
        let limit = if op == "book.put" { replica.limits.book_frame } else { replica.limits.frame };
        let frame = mg_live::gateway_frames::ClientFrame::Req(mg_live::gateway_frames::Request {
            id: u32::MAX,
            op: op.into(),
            a: a.clone(),
        });
        let bytes = serde_json::to_vec(&frame).map_err(|_| LinkError::Protocol)?;
        if bytes.len() > limit {
            return Err(LinkError::TooLarge);
        }
        let permit =
            self.shared.capacity.clone().try_acquire_owned().map_err(|_| LinkError::Busy)?;
        let (reply, receiver) = mpsc::channel();
        self.outgoing
            .try_send(Outgoing { op: op.into(), a, reply, _permit: permit })
            .map_err(|_| LinkError::Busy)?;
        Ok(PendingCall(receiver))
    }
    pub fn call(&self, op: &str, input: Value) -> Result<PendingCall, LinkError> {
        self.request("call", json!({"op":op,"input":input}))
    }
    pub fn set_watched_areas(&self, areas: &[String]) -> Result<PendingCall, LinkError> {
        let limit = self.shared.replica.lock().unwrap_or_else(|e| e.into_inner()).limits.watch;
        validate_watch(areas, limit)?;
        let areas: Vec<_> = areas.iter().map(|a| a.to_ascii_lowercase()).collect();
        let reply = self.request("watch.set", json!({"areas":areas}))?;
        *self.shared.watch.lock().unwrap_or_else(|e| e.into_inner()) = areas;
        Ok(reply)
    }
    pub fn stop(mut self) {
        self.shutdown();
    }
    fn shutdown(&mut self) {
        let _ = self.stop.send(true);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for LinkService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn validate_watch(areas: &[String], limit: usize) -> Result<(), LinkError> {
    if areas.len() > limit
        || areas.iter().any(|a| {
            a.is_empty()
                || a.len() > 16
                || !a.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
    {
        Err(LinkError::Protocol)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admission_bounds_queued_and_in_flight_together_and_releases_permits() {
        let (service, mut receiver) = fixture();
        let mut pending = Vec::new();
        for _ in 0..64 {
            pending.push(service.call("status", json!({})).unwrap());
        }
        assert_eq!(service.call("status", json!({})).unwrap_err(), LinkError::Busy);
        let in_flight = receiver.try_recv().unwrap();
        assert_eq!(service.call("status", json!({})).unwrap_err(), LinkError::Busy);
        drop(in_flight);
        assert!(service.call("status", json!({})).is_ok());
        assert_eq!(
            service.request("book.put", json!({"text":"x".repeat(262144)})).unwrap_err(),
            LinkError::TooLarge
        );
    }
}

#[cfg(test)]
pub(crate) fn fixture() -> (LinkService, tokio::sync::mpsc::Receiver<Outgoing>) {
    let mut replica = Replica::default();
    replica.snapshot.status = LinkStatus::Connected;
    let shared = Arc::new(Shared {
        replica: Mutex::new(replica),
        store: Arc::new(LiveStore::new_mirror()),
        watch: Mutex::new(Vec::new()),
        capacity: Arc::new(Semaphore::new(64)),
        wake: Arc::new(|| {}),
    });
    let (outgoing, receiver) = tokio::sync::mpsc::channel(64);
    let (stop, _) = tokio::sync::watch::channel(false);
    let service = LinkService {
        config: LinkConfig::new("test", Gateway::new("https://example.invalid", None).unwrap()),
        shared,
        outgoing,
        stop,
        worker: None,
    };
    (service, receiver)
}
