//! Owned background presence for discovered sessions, independent of MCP requests/jobs.
use crate::discovery::Descriptor;
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const PERIOD: Duration = Duration::from_secs(20);
enum Message {
    Sessions(Vec<Descriptor>),
    Stop,
}

pub struct Heartbeat {
    sender: mpsc::Sender<Message>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Heartbeat {
    pub fn start() -> Self {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            let mut sessions: Vec<Descriptor> = Vec::new();
            let mut next = Instant::now() + PERIOD;
            loop {
                match receiver.recv_timeout(next.saturating_duration_since(Instant::now())) {
                    Ok(Message::Sessions(updated)) => sessions = updated,
                    Ok(Message::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        for session in &sessions {
                            // No logs, stdout, discovery changes or surfaced failures.
                            session.ping();
                            match receiver.try_recv() {
                                Ok(Message::Stop) | Err(mpsc::TryRecvError::Disconnected) => return,
                                Ok(Message::Sessions(updated)) => {
                                    sessions = updated;
                                    break;
                                }
                                Err(mpsc::TryRecvError::Empty) => {}
                            }
                        }
                        next = Instant::now() + PERIOD;
                    }
                }
            }
        });
        Self { sender, worker: Some(worker) }
    }
    pub fn sessions(&self, sessions: Vec<Descriptor>) {
        let _ = self.sender.send(Message::Sessions(sessions));
    }
}
impl Drop for Heartbeat {
    fn drop(&mut self) {
        let _ = self.sender.send(Message::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
