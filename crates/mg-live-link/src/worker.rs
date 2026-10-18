use crate::{
    LinkConfig, LinkError, LinkStatus, SessionToken,
    link::{Outgoing, Shared},
    state,
    websocket::{self, Message},
};
use mg_live::{
    gateway::CommandsReply,
    gateway_frames::{ClientFrame, Hello, Request, ServerFrame},
    store::ErrorCode,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::AsyncWrite,
    sync::{OwnedSemaphorePermit, mpsc, watch},
    time::{Instant, timeout},
};

struct Pending {
    at: Instant,
    outgoing: Option<Outgoing>,
    commands: Vec<String>,
    _permit: Option<OwnedSemaphorePermit>,
}
impl Pending {
    fn finish(self, result: Result<Value, LinkError>, shared: &Shared) -> Result<(), LinkError> {
        if !self.commands.is_empty() {
            match &result {
                Ok(value) => {
                    let reply: CommandsReply =
                        serde_json::from_value(value.clone()).map_err(|_| LinkError::Protocol)?;
                    let mut seen = BTreeSet::new();
                    for id in reply.accepted.iter().chain(reply.rejected.iter().map(|r| &r.id)) {
                        if !self.commands.contains(id) || !seen.insert(id.clone()) {
                            return Err(LinkError::Protocol);
                        }
                    }
                    if seen.len() != self.commands.len() {
                        return Err(LinkError::Protocol);
                    }
                    for rejected in reply.rejected {
                        let code = serde_json::from_value::<ErrorCode>(json!(rejected.code))
                            .unwrap_or(ErrorCode::Invalid);
                        shared
                            .store
                            .mirror_reject(&rejected.id, code, "Gateway rejected command")
                            .map_err(|_| LinkError::Protocol)?;
                    }
                }
                Err(error) => {
                    let code = if *error == LinkError::Timeout {
                        ErrorCode::Unknown
                    } else {
                        ErrorCode::Invalid
                    };
                    for id in &self.commands {
                        shared
                            .store
                            .mirror_reject(id, code, "Gateway command outcome unavailable")
                            .map_err(|_| LinkError::Protocol)?;
                    }
                }
            }
        }
        if let Some(outgoing) = self.outgoing {
            if outgoing.op == "llm.allow" && result.is_ok() {
                shared.replica.lock().unwrap_or_else(|e| e.into_inner()).snapshot.llm_allowed =
                    outgoing.a["on"].as_bool();
            }
            let _ = outgoing.reply.send(result);
        }
        (shared.wake)();
        Ok(())
    }
}

pub(crate) async fn run(
    config: LinkConfig,
    token: SessionToken,
    shared: Arc<Shared>,
    mut outgoing: mpsc::Receiver<Outgoing>,
    mut stop: watch::Receiver<bool>,
) {
    let mut failures = 0;
    loop {
        let result = tokio::select! {
            _=stop.changed()=>Err(LinkError::Stopped),
            result=connection(&config,&token,&shared,&mut outgoing)=>result,
        };
        let error = result.err().unwrap_or(LinkError::Transport);
        // Never replay arbitrary RPCs: a lost response to a write is ambiguous.
        while let Ok(request) = outgoing.try_recv() {
            let _ = request.reply.send(Err(error));
        }
        let terminal = matches!(
            error,
            LinkError::Stopped
                | LinkError::PinMismatch
                | LinkError::Revoked
                | LinkError::RoleChanged
                | LinkError::Upgrade
                | LinkError::Replaced
                | LinkError::Protocol
                | LinkError::TooLarge
        );
        let was_connected = {
            let mut replica = shared.replica.lock().unwrap_or_else(|e| e.into_inner());
            let connected = replica.snapshot.status == LinkStatus::Connected;
            replica.snapshot.status =
                if terminal { LinkStatus::Stopped } else { LinkStatus::Reconnecting };
            replica.snapshot.error = Some(error);
            connected
        };
        (shared.wake)();
        if terminal {
            break;
        }
        if was_connected {
            failures = 0;
        }
        let delay = backoff(failures);
        failures = failures.saturating_add(1);
        tokio::select! {
            _=stop.changed()=>break,
            _=tokio::time::sleep(delay)=>{},
        }
        // Expire only never-sent commands, including while disconnected.
        let _ = shared.store.mirror_commands(mg_live::now_ms());
    }
    shared.replica.lock().unwrap_or_else(|e| e.into_inner()).snapshot.status = LinkStatus::Stopped;
    (shared.wake)();
}

fn backoff(attempt: u32) -> Duration {
    let ceiling = 1000u64.saturating_mul(1u64 << attempt.min(4)).min(10_000);
    let mut random = [0; 4];
    let jitter = if getrandom::fill(&mut random).is_ok() {
        u64::from(u32::from_ne_bytes(random)) % 401
    } else {
        200
    };
    Duration::from_millis((ceiling * (800 + jitter) / 1000).min(10_000))
}

struct AbortReader(tokio::task::JoinHandle<()>);
impl Drop for AbortReader {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn send<W: AsyncWrite + Unpin>(
    writer: &mut W,
    frame: ClientFrame,
    shared: &Shared,
) -> Result<(), LinkError> {
    let bytes = serde_json::to_vec(&frame).map_err(|_| LinkError::Protocol)?;
    let limit = {
        let replica = shared.replica.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(&frame,ClientFrame::Req(r) if r.op=="book.put") {
            replica.limits.book_frame
        } else {
            replica.limits.frame
        }
    };
    if bytes.len() > limit {
        return Err(LinkError::TooLarge);
    }
    timeout(Duration::from_secs(5), websocket::write(writer, 1, &bytes))
        .await
        .map_err(|_| LinkError::Timeout)?
}

async fn connection(
    config: &LinkConfig,
    token: &SessionToken,
    shared: &Arc<Shared>,
    outgoing: &mut mpsc::Receiver<Outgoing>,
) -> Result<(), LinkError> {
    let stream = timeout(config.connect_timeout, websocket::connect(&config.gateway, token))
        .await
        .map_err(|_| LinkError::Transport)??;
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (tx, mut rx) = mpsc::channel(1);
    let _reader = AbortReader(tokio::spawn(async move {
        let mut fragments = None;
        loop {
            let result = websocket::read(&mut reader, &mut fragments).await;
            let end = result.is_err() || matches!(result, Ok(Message::Close));
            if tx.send(result).await.is_err() || end {
                break;
            }
        }
    }));
    let resume = shared.replica.lock().unwrap_or_else(|e| e.into_inner()).resume.clone();
    let watch = shared.watch.lock().unwrap_or_else(|e| e.into_inner()).clone();
    send(
        &mut writer,
        ClientFrame::Hello(Hello {
            proto: 1,
            mirror: mg_live::MIRROR_VERSION,
            client: format!("moonglow/{}", env!("CARGO_PKG_VERSION")),
            resume,
            watch,
        }),
        shared,
    )
    .await?;
    let mut pending = BTreeMap::<u32, Pending>::new();
    let mut sent = BTreeSet::new();
    let mut next = 1u32;
    let mut welcomed = false;
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let started = Instant::now();
    let mut activity = started;
    let mut last_ack = started;
    let result=async {
        loop {
            tokio::select! {
                received=rx.recv()=> {
                    activity=Instant::now();
                    match received.ok_or(LinkError::Transport)?? {
                        Message::Text(bytes)=> {
                            let Some(frame)=state::decode(&bytes,token.expose())? else {
                                let mut replica=shared.replica.lock().unwrap_or_else(|e|e.into_inner());
                                replica.snapshot.ignored_frames=replica.snapshot.ignored_frames.saturating_add(1);
                                continue;
                            };
                            match frame {
                                ServerFrame::Welcome(w) if !welcomed=> {
                                    shared.replica.lock().unwrap_or_else(|e|e.into_inner()).welcome(&shared.store,w)?;
                                    welcomed=true;
                                }
                                ServerFrame::Push(push) if welcomed=> {
                                    shared.replica.lock().unwrap_or_else(|e|e.into_inner()).push(&shared.store,push,mg_live::now_ms())?;
                                }
                                ServerFrame::Res(response) if welcomed=> {
                                    if response.id==0||response.id>=next {return Err(LinkError::Protocol);}
                                    if let Some(request)=pending.remove(&response.id) {
                                        request.finish(response.result.map_err(|e|match e.code.as_str(){
                                            "forbidden"=>LinkError::Forbidden,"conflict"=>LinkError::Conflict,
                                            "unsupported"=>LinkError::Unsupported,_=>LinkError::Rejected,
                                        }),shared)?;
                                    }
                                }
                                ServerFrame::Bye(bye)=>return Err(match bye.code.as_str() {
                                    "revoked"=>LinkError::Revoked,"role_changed"=>LinkError::RoleChanged,
                                    "upgrade"=>LinkError::Upgrade,"replaced"=>LinkError::Replaced,
                                    "overloaded"|"shutdown"=>LinkError::Transport,_=>LinkError::Protocol,
                                }),
                                _=>return Err(LinkError::Protocol),
                            }
                            (shared.wake)();
                        }
                        Message::Ping(bytes)=>timeout(Duration::from_secs(5),websocket::write(&mut writer,10,&bytes)).await.map_err(|_|LinkError::Timeout)??,
                        Message::Pong=>{},
                        Message::Close=>{
                            let _=timeout(Duration::from_secs(1),websocket::write(&mut writer,8,&[])).await;
                            return Err(LinkError::Transport);
                        }
                    }
                }
                _=tick.tick()=> {
                    if (!welcomed&&started.elapsed()>config.connect_timeout)||activity.elapsed()>Duration::from_secs(45) {return Err(LinkError::Transport);}
                    // Retiring an ID without closing would let more than 64
                    // unresolved requests accumulate at the peer. A timeout is
                    // ambiguous and must not overwrite an already pushed result.
                    if pending.values().any(|p|p.at.elapsed()>config.request_timeout) {return Err(LinkError::Timeout);}
                    if !welcomed {continue;}
                    let limit=shared.replica.lock().unwrap_or_else(|e|e.into_inner()).limits.in_flight;
                    while pending.len()<limit {
                        if next>100_000 {return Err(LinkError::Transport);}
                        let (op,a,request)=if let Ok(out)=outgoing.try_recv() {
                            (out.op.clone(),out.a.clone(),Pending{at:Instant::now(),outgoing:Some(out),commands:Vec::new(),_permit:None})
                        } else {
                            let commands=shared.store.mirror_commands(mg_live::now_ms()).map_err(|_|LinkError::Protocol)?;
                            let Some(command)=commands.into_iter().find(|c|!sent.contains(&c.command.id)) else {break;};
                            let Ok(permit)=shared.capacity.clone().try_acquire_owned() else {break;};
                            let id=command.command.id.clone();
                            let a=json!({"commands":[command]});
                            shared.store.mirror_mark_sent(std::slice::from_ref(&id)).map_err(|_|LinkError::Protocol)?;
                            sent.insert(id.clone());
                            ("cmds".into(),a,Pending{at:Instant::now(),outgoing:None,commands:vec![id],_permit:Some(permit)})
                        };
                        pending.insert(next,request);
                        send(&mut writer,ClientFrame::Req(Request{id:next,op,a}),shared).await?;
                        next+=1;
                    }
                }
            }
            let periodic=last_ack.elapsed()>=Duration::from_secs(1);
            let ack=shared.replica.lock().unwrap_or_else(|e|e.into_inner()).ack(periodic);
            if let Some(ack)=ack {
                send(&mut writer,ClientFrame::Ack(ack),shared).await?;
                last_ack=Instant::now();
            }
        }
    }.await;
    // Commands keep their stable IDs for same-epoch replay. Never retry a call.
    for (_, request) in pending {
        if let Some(out) = request.outgoing {
            let _ =
                out.reply.send(Err(result.as_ref().err().copied().unwrap_or(LinkError::Transport)));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agent_consent_changes_only_after_successful_gateway_response() {
        let (service, mut receiver) = crate::link::fixture();
        let _ticket = service.set_allow_agents(true).unwrap();
        assert_eq!(service.allow_agents(), None);
        Pending {
            at: Instant::now(),
            outgoing: Some(receiver.try_recv().unwrap()),
            commands: Vec::new(),
            _permit: None,
        }
        .finish(Ok(json!({})), &service.shared)
        .unwrap();
        assert_eq!(service.allow_agents(), Some(true));
        let _ticket = service.set_allow_agents(false).unwrap();
        Pending {
            at: Instant::now(),
            outgoing: Some(receiver.try_recv().unwrap()),
            commands: Vec::new(),
            _permit: None,
        }
        .finish(Err(LinkError::Forbidden), &service.shared)
        .unwrap();
        assert_eq!(service.allow_agents(), Some(true));
    }
    #[test]
    fn reconnect_jitter_stays_bounded_and_grows() {
        for _ in 0..100 {
            assert!(
                (Duration::from_millis(800)..=Duration::from_millis(1200)).contains(&backoff(0))
            );
            assert!((Duration::from_secs(8)..=Duration::from_secs(10)).contains(&backoff(99)));
        }
    }
}
