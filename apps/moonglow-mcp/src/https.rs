//! Small bounded HTTP client with an exact certificate pin from a descriptor.
use rustls::{
    DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use serde_json::Value;
use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    sync::Arc,
    time::Duration,
};

pub const LIMIT: usize = 16 * 1024 * 1024;
#[derive(Debug)]
struct Pin {
    expected: String,
    provider: rustls::crypto::CryptoProvider,
}
impl ServerCertVerifier for Pin {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let actual = ring::digest::digest(&ring::digest::SHA256, end_entity.as_ref())
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if actual != self.expected {
            return Err(rustls::Error::General("Certificate pin mismatch".into()));
        }
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// HTTP failures remain typed until the caller decides whether a schema refresh is safe.
pub struct Reply {
    pub status: u16,
    pub value: Value,
    pub etag: Option<String>,
}
impl Reply {
    pub fn json(self) -> Result<Value, String> {
        if !(200..300).contains(&self.status) {
            return Err(format!(
                "Agent HTTP {}: {}",
                self.status,
                self.value.get("error").unwrap_or(&self.value)
            ));
        }
        Ok(self.value)
    }
}

type Tls = std::io::BufReader<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>;
/// One sequential connection per descriptor. Reuse requires an explicit server opt-in.
#[derive(Default)]
pub struct Client {
    stream: Option<Tls>,
    identity: String,
    last: Option<std::time::Instant>,
}
impl Client {
    pub fn request(
        &mut self,
        endpoint: (&str, &str, &str),
        path: &str,
        body: Option<&Value>,
        etag: Option<&str>,
        timeout: Duration,
    ) -> Result<Reply, String> {
        self.request_inner(endpoint, path, body, etag, timeout, None)
    }
    pub fn gateway_request(
        &mut self,
        endpoint: (&str, &str, &str),
        path: &str,
        body: Option<&Value>,
        etag: Option<&str>,
        timeout: Duration,
    ) -> Result<Reply, String> {
        let gateway =
            mg_live_link::Gateway::new(endpoint.0, (!endpoint.2.is_empty()).then_some(endpoint.2))
                .map_err(|e| e.to_string())?;
        // Trust must be decided before constructing a request containing the bearer token.
        let tls = gateway.tls_config().map_err(|e| e.to_string())?;
        let path = path.strip_prefix("/v1/").ok_or("Invalid agent endpoint")?;
        let mut reply = self.request_inner(
            endpoint,
            &format!("/agent/v1/{path}"),
            body,
            etag,
            timeout,
            Some(tls),
        )?;
        // A remote error body must never turn an echoed credential into a diagnostic.
        if !(200..300).contains(&reply.status) {
            reply.value = serde_json::json!({"error":"Gateway request refused"});
        }
        Ok(reply)
    }
    fn request_inner(
        &mut self,
        endpoint: (&str, &str, &str),
        path: &str,
        body: Option<&Value>,
        etag: Option<&str>,
        timeout: Duration,
        tls_config: Option<Arc<rustls::ClientConfig>>,
    ) -> Result<Reply, String> {
        let (url, token, pin) = endpoint;
        let authority = authority(
            url,
            token,
            if tls_config.is_some() && pin.is_empty() {
                "0000000000000000000000000000000000000000000000000000000000000000"
            } else {
                pin
            },
        )?;
        if !path.starts_with('/')
            || path.bytes().any(|b| !b.is_ascii_graphic())
            || etag.is_some_and(|e| !valid_etag(e))
        {
            return Err("Invalid HTTP path or revision".into());
        }
        let identity = format!("{url}/{pin}");
        // Core allows at least 30 seconds of idle keep-alive. Reconnect proactively,
        // never resend a possibly delivered mutation after a transport error.
        if self.identity != identity
            || self.last.is_none_or(|at| at.elapsed() >= Duration::from_secs(15))
        {
            self.stream = None;
        }
        let mut tls = match self.stream.take() {
            Some(tls) => tls,
            None => connect(&authority, pin, tls_config)?,
        };
        self.identity = identity;
        tls.get_mut().sock.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
        let data = body.map(Value::to_string).unwrap_or_default();
        let method = if body.is_some() { "POST" } else { "GET" };
        let conditional = etag.map(|e| format!("If-Match: {e}\r\n")).unwrap_or_default();
        // One TLS write avoids many small records and delayed-ACK stalls from write! fragments.
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n{conditional}\r\n{data}",
            data.len()
        );
        tls.get_mut()
            .write_all(request.as_bytes())
            .map_err(|e| format!("Agent write failed (not retried): {e}"))?;
        tls.get_mut().flush().map_err(|e| e.to_string())?;
        let (head, bytes) = read_message(&mut tls)
            .map_err(|e| format!("Agent response failed (request not retried): {e}"))?;
        let status = head
            .lines()
            .next()
            .and_then(|s| s.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u16>().ok())
            .ok_or("Invalid HTTP status")?;
        let value =
            serde_json::from_slice(&bytes).map_err(|e| format!("Invalid agent JSON: {e}"))?;
        let mut revision = None;
        let mut keep = false;
        for line in head.lines().skip(1) {
            if let Some((key, value)) = line.split_once(':') {
                let value = value.trim();
                if key.eq_ignore_ascii_case("etag") && valid_etag(value) {
                    revision = Some(value.into());
                }
                if key.eq_ignore_ascii_case("connection") {
                    keep = value.eq_ignore_ascii_case("keep-alive");
                }
            }
        }
        if keep {
            self.stream = Some(tls);
        }
        self.last = Some(std::time::Instant::now());
        Ok(Reply { status, value, etag: revision })
    }
}
pub fn valid_etag(e: &str) -> bool {
    e.len() >= 2
        && e.len() <= 256
        && e.starts_with('"')
        && e.ends_with('"')
        && e[1..e.len() - 1].bytes().all(|b| b.is_ascii_graphic() && b != b'"')
}
fn authority(url: &str, token: &str, pin: &str) -> Result<String, String> {
    let authority =
        url.strip_prefix("https://").ok_or("Descriptor URL must use HTTPS")?.trim_end_matches('/');
    if authority.contains(['/', '@', '?', '#', '\r', '\n'])
        || token.bytes().any(|b| !b.is_ascii_graphic())
        || token.is_empty()
        || pin.len() != 64
        || !pin.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Invalid descriptor URL, token or certificate pin".into());
    }
    Ok(authority.into())
}
fn connect(
    authority: &str,
    pin: &str,
    tls_config: Option<Arc<rustls::ClientConfig>>,
) -> Result<Tls, String> {
    let (host, port) = if let Some(ip) = authority.strip_prefix('[') {
        let (host, rest) = ip.split_once(']').ok_or("Invalid IPv6 URL")?;
        (
            host,
            if rest.is_empty() {
                443
            } else {
                rest.strip_prefix(':')
                    .ok_or("Invalid URL")?
                    .parse::<u16>()
                    .map_err(|e| e.to_string())?
            },
        )
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h, p.parse::<u16>().map_err(|e| e.to_string())?),
            None => (authority, 443),
        }
    };
    let mut socket = None;
    for address in (host, port).to_socket_addrs().map_err(|e| e.to_string())?.take(4) {
        if let Ok(s) = TcpStream::connect_timeout(&address, Duration::from_secs(2)) {
            socket = Some(s);
            break;
        }
    }
    let stream = socket.ok_or("Agent is unreachable")?;
    stream.set_nodelay(true).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    let provider = rustls::crypto::ring::default_provider();
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(provider.clone()))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pin {
            expected: pin.to_ascii_lowercase(),
            provider,
        }))
        .with_no_client_auth();
    let connection = rustls::ClientConnection::new(
        tls_config.unwrap_or_else(|| Arc::new(config)),
        ServerName::try_from(host.to_owned()).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(std::io::BufReader::new(rustls::StreamOwned::new(connection, stream)))
}

fn line(stream: &mut impl Read, limit: usize) -> Result<String, String> {
    let mut bytes = Vec::new();
    let mut byte = [0];
    while bytes.len() < limit {
        stream.read_exact(&mut byte).map_err(|e| e.to_string())?;
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n") {
            return String::from_utf8(bytes).map_err(|e| e.to_string());
        }
    }
    Err("HTTP line too large".into())
}
pub fn read_message(stream: &mut impl Read) -> Result<(String, Vec<u8>), String> {
    let mut head = String::new();
    loop {
        let next = line(stream, 16384)?;
        head.push_str(&next);
        if head.len() > 65536 {
            return Err("HTTP headers too large".into());
        }
        if next == "\r\n" {
            break;
        }
    }
    let mut length = None;
    let mut chunked = false;
    for header in head.lines().skip(1).filter(|s| !s.is_empty()) {
        let (key, value) = header.split_once(':').ok_or("Invalid HTTP header")?;
        if key.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err("Duplicate Content-Length".into());
            }
            length = Some(value.trim().parse::<usize>().map_err(|e| e.to_string())?);
        }
        if key.eq_ignore_ascii_case("transfer-encoding") {
            if value.trim().eq_ignore_ascii_case("chunked") {
                chunked = true;
            } else {
                return Err("Unsupported transfer encoding".into());
            }
        }
    }
    if chunked && length.is_some() {
        return Err("Conflicting HTTP body framing".into());
    }
    let mut body = Vec::new();
    if chunked {
        loop {
            let size_line = line(stream, 128)?;
            let size =
                usize::from_str_radix(size_line.trim().split(';').next().unwrap_or_default(), 16)
                    .map_err(|e| e.to_string())?;
            if size == 0 {
                let mut trailers = 0;
                loop {
                    let trailer = line(stream, 16384)?;
                    trailers += trailer.len();
                    if trailers > 65536 {
                        return Err("HTTP trailers too large".into());
                    }
                    if trailer == "\r\n" {
                        break;
                    }
                }
                break;
            }
            if size > LIMIT - body.len() {
                return Err("HTTP body too large".into());
            }
            let offset = body.len();
            body.resize(offset + size, 0);
            stream.read_exact(&mut body[offset..]).map_err(|e| e.to_string())?;
            if line(stream, 2)? != "\r\n" {
                return Err("Invalid HTTP chunk terminator".into());
            }
        }
    } else {
        let n = match length {
            Some(n) => n,
            None if head.starts_with("GET ") || head.starts_with("DELETE ") => 0,
            None => return Err("HTTP message needs a body length".into()),
        };
        if n > LIMIT {
            return Err("HTTP body too large".into());
        }
        body.resize(n, 0);
        stream.read_exact(&mut body).map_err(|e| e.to_string())?;
    }
    Ok((head, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn http_truncation_and_ambiguous_framing_are_errors() {
        for message in [
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".as_slice(),
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\n\r\n",
        ] {
            assert_eq!(read_message(&mut &message[..]).unwrap().1, b"{}");
            for n in 0..message.len() {
                assert!(read_message(&mut &message[..n]).is_err());
            }
        }
        for bad in [
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n{}",
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nTransfer-Encoding: chunked\r\n\r\n{}",
            "HTTP/1.1 200 OK\r\nContent-Length: 999999999\r\n\r\n",
        ] {
            assert!(read_message(&mut bad.as_bytes()).is_err());
        }
    }
}
