use std::fmt;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use url::Url;

use crate::tls::{self, Pinned, TrustPrompt};

/// Errors deliberately exclude request URLs, grants and response bodies.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Use an HTTPS gateway origin without credentials, path, query or fragment")]
    InvalidGateway,
    #[error("Certificate pin must be 64 hexadecimal characters")]
    InvalidPin,
    #[error("Trust the gateway certificate explicitly before signing in")]
    TrustRequired(TrustPrompt),
    #[error("Gateway certificate does not match the saved fingerprint")]
    PinMismatch,
    #[error("Could not reach or authenticate the gateway")]
    Transport,
    #[error("Login timed out")]
    Timeout,
    #[error("Invalid login callback")]
    Callback,
    #[error("Login state does not match")]
    State,
    #[error("System random source unavailable")]
    Random,
    #[error("Loopback listener unavailable")]
    Listener,
    #[error("Gateway returned an invalid session token")]
    Token,
}

/// A secret with redacted Debug output; never serialize it into settings.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionToken(String);

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionToken([redacted])")
    }
}

impl SessionToken {
    pub fn new(value: String) -> Result<Self, Error> {
        if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
            return Err(Error::Token);
        }
        Ok(Self(value))
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct Gateway {
    pub(crate) origin: Url,
    pub(crate) pin: Option<String>,
}

impl Gateway {
    pub fn new(origin: &str, pin: Option<&str>) -> Result<Self, Error> {
        let mut origin = Url::parse(origin).map_err(|_| Error::InvalidGateway)?;
        if origin.scheme() == "wss" {
            origin.set_scheme("https").map_err(|_| Error::InvalidGateway)?;
        }
        if origin.scheme() != "https"
            || origin.host_str().is_none()
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
        {
            return Err(Error::InvalidGateway);
        }
        if pin.is_some_and(|p| p.len() != 64 || !p.bytes().all(|b| b.is_ascii_hexdigit())) {
            return Err(Error::InvalidPin);
        }
        Ok(Self { origin, pin: pin.map(str::to_ascii_lowercase) })
    }

    /// Discover an untrusted IP gateway certificate, without sending any HTTP data.
    /// DNS gateways use public WebPKI roots; a saved pin always takes precedence.
    pub fn check_trust(&self, timeout: Duration) -> Result<(), Error> {
        if self.pin.is_some()
            || !matches!(self.origin.host(), Some(url::Host::Ipv4(_) | url::Host::Ipv6(_)))
        {
            return Ok(());
        }
        let deadline = Instant::now() + timeout;
        let observed = Arc::new(Mutex::new(None));
        let cfg = tls::config(Some(Pinned { pin: None, observed: observed.clone() }));
        let host = self.origin.host_str().ok_or(Error::InvalidGateway)?.trim_matches(['[', ']']);
        let addr = (host, self.origin.port_or_known_default().unwrap())
            .to_socket_addrs()
            .map_err(|_| Error::Transport)?
            .next()
            .ok_or(Error::Transport)?;
        let stream = TcpStream::connect_timeout(&addr, timeout).map_err(|_| Error::Transport)?;
        stream.set_read_timeout(Some(timeout)).map_err(|_| Error::Transport)?;
        stream.set_write_timeout(Some(timeout)).map_err(|_| Error::Transport)?;
        let name = rustls::pki_types::ServerName::try_from(host.to_owned())
            .map_err(|_| Error::InvalidGateway)?;
        let mut conn =
            rustls::ClientConnection::new(Arc::new(cfg), name).map_err(|_| Error::Transport)?;
        let _ = conn.complete_io(&mut DeadlineSocket { stream, deadline });
        let sha256 = observed.lock().unwrap().clone().ok_or_else(|| {
            if Instant::now() >= deadline { Error::Timeout } else { Error::Transport }
        })?;
        Err(Error::TrustRequired(TrustPrompt {
            origin: self.origin.origin().ascii_serialization(),
            sha256,
        }))
    }

    fn exchange(
        &self,
        grant: &str,
        verifier: &str,
        timeout: Duration,
    ) -> Result<SessionToken, Error> {
        let observed = Arc::new(Mutex::new(None));
        let verifier_config = self
            .pin
            .as_ref()
            .map(|pin| Pinned { pin: Some(pin.clone()), observed: observed.clone() });
        let agent = ureq::AgentBuilder::new()
            .redirects(0)
            .timeout(timeout)
            .tls_config(Arc::new(tls::config(verifier_config)))
            .build();
        let response = agent
            .post(self.origin.join("auth/token").unwrap().as_str())
            .send_json(serde_json::json!({"grant": grant, "verifier": verifier}));
        let response = response.map_err(|_| {
            if self.pin.is_some()
                && observed.lock().unwrap().as_ref().is_some_and(|p| Some(p) != self.pin.as_ref())
            {
                Error::PinMismatch
            } else {
                Error::Transport
            }
        })?;
        if response.status() != 200 {
            return Err(Error::Transport);
        }
        let mut bytes = Vec::new();
        response.into_reader().take(8193).read_to_end(&mut bytes).map_err(|_| Error::Transport)?;
        if bytes.len() > 8192 {
            return Err(Error::Token);
        }
        #[derive(serde::Deserialize)]
        struct Reply {
            token: String,
        }
        let reply: Reply = serde_json::from_slice(&bytes).map_err(|_| Error::Token)?;
        SessionToken::new(reply.token)
    }
}

/// Socket timeouts alone restart on each read; a slow peer must not extend discovery forever.
struct DeadlineSocket {
    stream: TcpStream,
    deadline: Instant,
}

impl DeadlineSocket {
    fn remaining(&self) -> std::io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::TimedOut))
    }
}

impl Read for DeadlineSocket {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.stream.set_read_timeout(Some(self.remaining()?))?;
        self.stream.read(buf)
    }
}

impl Write for DeadlineSocket {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        self.stream.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}

/// An owned, one-shot login attempt. Dropping it closes the listening socket.
pub struct Login {
    gateway: Gateway,
    listener: TcpListener,
    verifier: String,
    state: String,
    url: Url,
    deadline: Instant,
}

impl fmt::Debug for Login {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Login([redacted])")
    }
}

fn random() -> Result<String, Error> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| Error::Random)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

impl Login {
    /// Call on a worker: trust discovery may perform a bounded TLS handshake.
    pub fn begin(gateway: Gateway, timeout: Duration) -> Result<Self, Error> {
        let deadline = Instant::now() + timeout;
        gateway.check_trust(timeout.min(Duration::from_secs(10)))?;
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map_err(|_| Error::Listener)?;
        listener.set_nonblocking(true).map_err(|_| Error::Listener)?;
        let verifier = random()?;
        let state = random()?;
        let mut url = gateway.origin.join("auth/start").unwrap();
        url.query_pairs_mut()
            .append_pair("challenge", &URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())))
            .append_pair(
                "port",
                &listener.local_addr().map_err(|_| Error::Listener)?.port().to_string(),
            )
            .append_pair("state", &state);
        Ok(Self { gateway, listener, verifier, state, url, deadline })
    }

    pub fn browser_url(&self) -> &str {
        self.url.as_str()
    }

    /// Ignore bounded invalid probes, accept one valid callback, then close and redeem.
    pub fn finish(self) -> Result<SessionToken, Error> {
        let Self { gateway, listener, verifier, state, deadline, .. } = self;
        let mut attempts = 0;
        let (mut stream, grant) = loop {
            if Instant::now() >= deadline {
                return Err(Error::Timeout);
            }
            let mut stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(_) => return Err(Error::Listener),
            };
            attempts += 1;
            stream.set_nonblocking(false).map_err(|_| Error::Callback)?;
            let connection_deadline = deadline.min(Instant::now() + Duration::from_millis(250));
            let result = callback(&mut stream, &state, connection_deadline);
            if let Ok(grant) = result {
                break (stream, grant);
            }
            callback_response(&mut stream, false, connection_deadline);
            if attempts >= 32 {
                return Err(Error::Callback);
            }
        };
        drop(listener);
        callback_response(&mut stream, true, deadline);
        drop(stream);
        let remaining = deadline.checked_duration_since(Instant::now()).ok_or(Error::Timeout)?;
        gateway.exchange(&grant, &verifier, remaining.min(Duration::from_secs(15)))
    }
}

fn callback_response(stream: &mut TcpStream, accepted: bool, deadline: Instant) {
    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else { return };
    if stream.set_write_timeout(Some(remaining.min(Duration::from_millis(250)))).is_err() {
        return;
    }
    let (status, body) = if accepted {
        ("200 OK", "Sign-in received. You can close this window.")
    } else {
        ("400 Bad Request", "Invalid callback. Return to Moonglow and continue signing in.")
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
}

fn callback(stream: &mut TcpStream, state: &str, deadline: Instant) -> Result<String, Error> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let remaining = deadline.checked_duration_since(Instant::now()).ok_or(Error::Timeout)?;
        stream.set_read_timeout(Some(remaining)).map_err(|_| Error::Callback)?;
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(1) => header.push(byte[0]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Err(Error::Timeout);
            }
            _ => return Err(Error::Callback),
        }
        if header.len() > 8192 {
            return Err(Error::Callback);
        }
    }
    let header = std::str::from_utf8(&header).map_err(|_| Error::Callback)?;
    let mut request = header.lines().next().ok_or(Error::Callback)?.split_whitespace();
    if request.next() != Some("GET") {
        return Err(Error::Callback);
    }
    let target = request.next().ok_or(Error::Callback)?;
    if !target.starts_with("/cb?") || request.next() != Some("HTTP/1.1") || request.next().is_some()
    {
        return Err(Error::Callback);
    }
    let url = Url::parse(&format!("http://127.0.0.1{target}")).map_err(|_| Error::Callback)?;
    let mut states = Vec::new();
    let mut grants = Vec::new();
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "state" => states.push(value.into_owned()),
            "grant" => grants.push(value.into_owned()),
            _ => {}
        }
    }
    if states.len() != 1 || states[0] != state {
        return Err(Error::State);
    }
    if grants.len() != 1 || grants[0].is_empty() || url.fragment().is_some() {
        return Err(Error::Callback);
    }
    Ok(grants.remove(0))
}
