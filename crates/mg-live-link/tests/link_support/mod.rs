use mg_gateway::{Authenticator, Config, CoreHandle, Principal};
use mg_live::store::{LiveStore, Policy};
use mg_live_link::{
    Gateway, LinkConfig, PendingCall, SessionToken,
    credentials::{Backend, StoreError, TokenStore},
};
use serde_json::Value;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::watch,
    task::{JoinHandle, JoinSet},
};

pub(crate) const SECRET: &str = "fixture-session-credential";
pub(crate) struct Memory;
impl Backend for Memory {
    fn get(&self, _: &str) -> Result<Option<SessionToken>, StoreError> {
        Ok(Some(SessionToken::new(SECRET.into()).unwrap()))
    }
    fn set(&self, _: &str, _: &SessionToken) -> Result<(), StoreError> {
        Ok(())
    }
    fn delete(&self, _: &str) -> Result<(), StoreError> {
        Ok(())
    }
}
pub(crate) fn tokens() -> TokenStore<Memory> {
    TokenStore::new(Memory, std::path::Path::new("unused-test-credentials"))
}
#[derive(Debug)]
struct Auth;
impl Authenticator for Auth {
    fn authenticate(&self, bearer: &str) -> Option<Principal> {
        (bearer == SECRET).then(|| Principal {
            id: "dm:test".into(),
            name: "Test".into(),
            groups: vec!["read".into(), "read_chat".into(), "npc".into(), "focus".into()],
        })
    }
}
pub(crate) struct Host {
    pub(crate) origin: String,
    pub(crate) pin: String,
    pub(crate) store: Arc<LiveStore>,
    pub(crate) core: CoreHandle,
    tasks: Vec<JoinHandle<()>>,
    kill: watch::Sender<u64>,
    fault: watch::Sender<Option<Vec<u8>>>,
    frames: Arc<Mutex<Vec<Value>>>,
}
impl Host {
    pub(crate) async fn new() -> Self {
        let store = Arc::new(LiveStore::default());
        let core = CoreHandle::new(store.clone()).await.unwrap();
        let router = mg_gateway::router(
            core.clone(),
            Arc::new(Auth),
            Arc::new(Policy::default()),
            Config::default(),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let (tls, pin) = tls();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("https://{}", listener.local_addr().unwrap());
        let (kill, kill_rx) = watch::channel(0);
        let (fault, fault_rx) = watch::channel(None::<Vec<u8>>);
        let frames = Arc::new(Mutex::new(Vec::new()));
        let recorded = frames.clone();
        let proxy = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    accepted=listener.accept()=>{
                        let (socket,_)=accepted.unwrap();
                        let tls=tls.clone();let frames=recorded.clone();let mut kill=kill_rx.clone();kill.borrow_and_update();let mut fault=fault_rx.clone();fault.borrow_and_update();
                        connections.spawn(async move {
                            let work=async move {
                                let Ok(tls)=tls.accept(socket).await else{return;};
                                let Ok(tcp)=tokio::net::TcpStream::connect(address).await else{return;};
                                let (mut client_read,mut client_write)=tokio::io::split(tls);
                                let (mut server_read,mut server_write)=tcp.into_split();
                                let up=async {
                                    // Header bytes are forwarded, never retained in trace records.
                                    let mut header=Vec::new();
                                    while !header.ends_with(b"\r\n\r\n") {
                                        if header.len()>16384{return Ok::<(),std::io::Error>(());}
                                        header.push(client_read.read_u8().await?);
                                    }
                                    server_write.write_all(&header).await?;
                                    drop(header);
                                    loop {
                                        let mut bytes=vec![0;2];client_read.read_exact(&mut bytes).await?;
                                        let short=bytes[1]&127;
                                        let extra=match short{126=>2,127=>8,_=>0};
                                        let mut size=vec![0;extra];client_read.read_exact(&mut size).await?;
                                        let len=match short {126=>u16::from_be_bytes(size[..].try_into().unwrap()) as usize,127=>u64::from_be_bytes(size[..].try_into().unwrap()) as usize,n=>n as usize};
                                        if len>262144{return Ok(());}
                                        bytes.extend(size);
                                        let mut mask=[0;4];client_read.read_exact(&mut mask).await?;bytes.extend(mask);
                                        let mut body=vec![0;len];client_read.read_exact(&mut body).await?;bytes.extend_from_slice(&body);
                                        for (i,b) in body.iter_mut().enumerate(){*b^=mask[i%4];}
                                        if bytes[0]&15==1 {frames.lock().unwrap().push(serde_json::from_slice(&body).unwrap());}
                                        server_write.write_all(&bytes).await?;
                                    }
                                };
                                let down=async {
                                    tokio::select! {
                                        _=tokio::io::copy(&mut server_read,&mut client_write)=>{},
                                        _=fault.changed()=> {
                                            let bytes=fault.borrow().clone();
                                            if let Some(bytes)=bytes {let _=client_write.write_all(&bytes).await;}
                                            tokio::time::sleep(Duration::from_secs(2)).await;
                                        }
                                    }
                                };
                                tokio::select!{_=up=>{},_=down=>{}}
                            };
                            tokio::select!{_=work=>{},_=kill.changed()=>{}}
                        });
                    }
                    _=connections.join_next(),if !connections.is_empty()=>{},
                }
            }
        });
        Self { origin, pin, store, core, tasks: vec![server, proxy], kill, fault, frames }
    }
    pub(crate) fn config(&self) -> LinkConfig {
        let mut config =
            LinkConfig::new("test", Gateway::new(&self.origin, Some(&self.pin)).unwrap());
        config.watch = vec!["square".into()];
        config
    }
    pub(crate) fn frames(&self) -> Vec<Value> {
        self.frames.lock().unwrap().clone()
    }
    pub(crate) fn drop_sockets(&self) {
        self.kill.send_modify(|n| *n += 1);
    }
    pub(crate) fn inject(&self, bytes: Vec<u8>) {
        let _ = self.fault.send(Some(bytes));
    }
    pub(crate) async fn report(&self, seq: u64, events: Value, results: Value) {
        self.core.with_store(move|s|s.sync(serde_json::json!({"v":1,"boot":"boot","seq":seq,
            "areas":[{"area":"square","full":true,"cre":[{"id":"a1","name":format!("Guard {seq}"),"r":"guard","hp":[5,9]}]}],
            "events":events,"results":results}).to_string().as_bytes(),mg_live::now_ms()).unwrap()).await.unwrap();
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.core.request_shutdown();
        for task in &self.tasks {
            task.abort();
        }
    }
}
pub(crate) fn tls() -> (tokio_rustls::TlsAcceptor, String) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let der = cert.cert.der().clone();
    let pin = mg_live_link::fingerprint(&der);
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![der], key.into())
    .unwrap();
    (tokio_rustls::TlsAcceptor::from(Arc::new(config)), pin)
}
pub(crate) async fn until(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(8), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("condition deadline");
}
pub(crate) async fn reply_async(call: PendingCall) -> Result<Value, mg_live_link::LinkError> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(value) = call.try_recv()? {
                return Ok(value);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
