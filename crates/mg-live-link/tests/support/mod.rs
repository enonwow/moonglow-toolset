#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, mpsc};
use std::time::Duration;

pub(crate) struct FakeGateway {
    pub(crate) url: String,
    pub(crate) pin: String,
    pub(crate) request: mpsc::Receiver<serde_json::Value>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl FakeGateway {
    pub(crate) fn start() -> Self {
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let pin = mg_live_link::fingerprint(certified.cert.der().as_ref());
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![certified.cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()).into(),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("https://{}", listener.local_addr().unwrap());
        let (tx, request) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let stream = loop {
                if std::time::Instant::now() > deadline {
                    return;
                }
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            stream.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut stream = rustls::StreamOwned::new(
                rustls::ServerConnection::new(Arc::new(config)).unwrap(),
                stream,
            );
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                if stream.read_exact(&mut byte).is_err() {
                    return;
                }
                header.push(byte[0]);
                assert!(header.len() < 8192);
            }
            let header = String::from_utf8(header).unwrap();
            assert!(header.starts_with("POST /auth/token HTTP/1.1\r\n"));
            let len: usize = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|s| s.trim().parse().unwrap())
                })
                .unwrap();
            let mut body = vec![0; len];
            stream.read_exact(&mut body).unwrap();
            tx.send(serde_json::from_slice(&body).unwrap()).unwrap();
            let body = r#"{"token":"fixture-session"}"#;
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            stream.flush().unwrap();
            stream.conn.send_close_notify();
            let _ = stream.flush();
        });
        Self { url, pin, request, worker: Some(worker) }
    }

    pub(crate) fn finish(mut self) {
        self.worker.take().unwrap().join().unwrap();
    }
}

pub(crate) fn callback(browser_url: &str, wrong_state: bool) -> (u16, String) {
    let url = url::Url::parse(browser_url).unwrap();
    let fields: std::collections::HashMap<_, _> = url.query_pairs().collect();
    let port: u16 = fields["port"].parse().unwrap();
    let state = if wrong_state { "wrong-state" } else { &fields["state"] };
    let mut socket = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    write!(
        socket,
        "GET /cb?grant=fixture-grant&state={state} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).unwrap();
    (port, response)
}
