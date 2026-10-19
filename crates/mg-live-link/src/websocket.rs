//! Small RFC 6455 transport. We own the upgrade because generic WebSocket
//! handshakes can trace raw Authorization headers (including sensitive headers).
//! No compression, redirects, cookies, proxy credentials or raw-wire logging.
use crate::{Gateway, LinkError, SessionToken, tls};
use base64::{Engine, engine::general_purpose::STANDARD};
use sha1::{Digest, Sha1};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

const MAX: usize = mg_live::gateway_frames::SERVER_FRAME_LIMIT;
pub(crate) trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}
/// TLS, or plain TCP to a loopback development gateway (`Gateway::new` allows nothing else).
pub(crate) type Stream = Box<dyn Io>;

pub(crate) async fn connect(gateway: &Gateway, token: &SessionToken) -> Result<Stream, LinkError> {
    let host = gateway.origin.host_str().ok_or(LinkError::Protocol)?.trim_matches(['[', ']']);
    let port = gateway.origin.port_or_known_default().ok_or(LinkError::Protocol)?;
    let observed = Arc::new(Mutex::new(None));
    let verifier = gateway
        .pin
        .as_ref()
        .map(|pin| tls::Pinned { pin: Some(pin.clone()), observed: observed.clone() });
    let config = Arc::new(tls::config(verifier));
    let stream = TcpStream::connect((host, port)).await.map_err(|_| LinkError::Transport)?;
    stream.set_nodelay(true).map_err(|_| LinkError::Transport)?;
    if gateway.origin.scheme() == "http" {
        let mut stream: Stream = Box::new(stream);
        upgrade(&mut stream, gateway, token).await?;
        return Ok(stream);
    }
    let name = rustls::pki_types::ServerName::try_from(host.to_owned())
        .map_err(|_| LinkError::Protocol)?;
    let stream = TlsConnector::from(config).connect(name, stream).await.map_err(|_| {
        if gateway.pin.is_some()
            && observed
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .is_some_and(|p| Some(p) != gateway.pin.as_ref())
        {
            LinkError::PinMismatch
        } else {
            LinkError::Transport
        }
    })?;
    let mut stream: Stream = Box::new(stream);
    upgrade(&mut stream, gateway, token).await?;
    Ok(stream)
}

async fn upgrade(
    stream: &mut Stream,
    gateway: &Gateway,
    token: &SessionToken,
) -> Result<(), LinkError> {
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).map_err(|_| LinkError::Transport)?;
    let key = STANDARD.encode(nonce);
    let authority = gateway.origin[url::Position::BeforeHost..url::Position::AfterPort].to_owned();
    let mirror = mg_live::MIRROR_VERSION;
    let request = format!(
        "GET /ws/v1?proto=1&mirror={mirror} HTTP/1.1\r\nHost: {authority}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Protocol: moonglow-gw.1\r\nAuthorization: Bearer {}\r\n\r\n",
        token.expose()
    );
    stream.write_all(request.as_bytes()).await.map_err(|_| LinkError::Transport)?;
    stream.flush().await.map_err(|_| LinkError::Transport)?;
    let mut response = Vec::new();
    while !response.ends_with(b"\r\n\r\n") {
        if response.len() >= 16384 {
            return Err(LinkError::TooLarge);
        }
        response.push(stream.read_u8().await.map_err(|_| LinkError::Transport)?);
    }
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut parsed = httparse::Response::new(&mut headers);
    parsed.parse(&response).map_err(|_| LinkError::Protocol)?;
    match parsed.code {
        Some(101) => {}
        Some(401 | 403) => return Err(LinkError::Revoked),
        Some(426) => return Err(LinkError::Upgrade),
        _ => return Err(LinkError::Transport),
    }
    let header = |name: &str| -> Result<&[u8], LinkError> {
        let mut values = parsed.headers.iter().filter(|h| h.name.eq_ignore_ascii_case(name));
        let value = values.next().ok_or(LinkError::Protocol)?.value;
        if values.next().is_some() {
            return Err(LinkError::Protocol);
        }
        Ok(value)
    };
    let accept = STANDARD
        .encode(Sha1::digest(format!("{key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11").as_bytes()));
    let connection = std::str::from_utf8(header("connection")?).map_err(|_| LinkError::Protocol)?;
    if parsed.version != Some(1)
        || !header("upgrade")?.eq_ignore_ascii_case(b"websocket")
        || !connection.split(',').any(|s| s.trim().eq_ignore_ascii_case("upgrade"))
        || header("sec-websocket-accept")? != accept.as_bytes()
        || header("sec-websocket-protocol")? != b"moonglow-gw.1"
        || parsed.headers.iter().any(|h| h.name.eq_ignore_ascii_case("sec-websocket-extensions"))
    {
        return Err(LinkError::Protocol);
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) enum Message {
    Text(Vec<u8>),
    Ping(Vec<u8>),
    Pong,
    Close,
}

/// This future must not be cancelled and restarted midway through a frame.
/// The service runs it in a dedicated task and receives complete messages.
pub(crate) async fn read<R: AsyncRead + Unpin>(
    stream: &mut R,
    fragments: &mut Option<Vec<u8>>,
) -> Result<Message, LinkError> {
    loop {
        let mut header = [0; 2];
        stream.read_exact(&mut header).await.map_err(|_| LinkError::Transport)?;
        let fin = header[0] & 128 != 0;
        let op = header[0] & 15;
        if header[0] & 112 != 0 || header[1] & 128 != 0 || !matches!(op, 0 | 1 | 8 | 9 | 10) {
            return Err(LinkError::Protocol);
        }
        let short = header[1] & 127;
        let len = match short {
            126 => u64::from(stream.read_u16().await.map_err(|_| LinkError::Transport)?),
            127 => stream.read_u64().await.map_err(|_| LinkError::Transport)?,
            n => u64::from(n),
        };
        if (short == 126 && len < 126)
            || (short == 127 && (len < 65536 || len >> 63 != 0))
            || (op >= 8 && (!fin || len > 125))
        {
            return Err(LinkError::Protocol);
        }
        if len > MAX as u64
            || (op < 8
                && len.saturating_add(fragments.as_ref().map_or(0, |v| v.len()) as u64)
                    > MAX as u64)
        {
            return Err(LinkError::TooLarge);
        }
        let mut body = vec![0; len as usize];
        stream.read_exact(&mut body).await.map_err(|_| LinkError::Transport)?;
        match op {
            8 => {
                if body.len() == 1 || (body.len() > 2 && std::str::from_utf8(&body[2..]).is_err()) {
                    return Err(LinkError::Protocol);
                }
                return Ok(Message::Close);
            }
            9 => return Ok(Message::Ping(body)),
            10 => return Ok(Message::Pong),
            1 if fragments.is_none() => *fragments = Some(body),
            0 => fragments.as_mut().ok_or(LinkError::Protocol)?.extend(body),
            _ => return Err(LinkError::Protocol),
        }
        if fin {
            let text = fragments.take().ok_or(LinkError::Protocol)?;
            std::str::from_utf8(&text).map_err(|_| LinkError::Protocol)?;
            return Ok(Message::Text(text));
        }
    }
}

pub(crate) async fn write<W: AsyncWrite + Unpin>(
    stream: &mut W,
    op: u8,
    bytes: &[u8],
) -> Result<(), LinkError> {
    let mut mask = [0; 4];
    getrandom::fill(&mut mask).map_err(|_| LinkError::Transport)?;
    let mut frame = Vec::with_capacity(bytes.len() + 14);
    frame.push(128 | op);
    if bytes.len() < 126 {
        frame.push(128 | bytes.len() as u8);
    } else if bytes.len() <= 65535 {
        frame.push(128 | 126);
        frame.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    } else {
        frame.push(128 | 127);
        frame.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    }
    frame.extend(mask);
    frame.extend(bytes.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    stream.write_all(&frame).await.map_err(|_| LinkError::Transport)?;
    stream.flush().await.map_err(|_| LinkError::Transport)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn fragment_control_interleave_and_client_masking() {
        let bytes = [1, 2, b'{', b'"', 137, 1, b'p', 128, 5, b'a', b'"', b':', b'0', b'}'];
        let mut reader = bytes.as_slice();
        let mut fragments = None;
        assert!(
            matches!(read(&mut reader, &mut fragments).await.unwrap(), Message::Ping(v) if v==b"p")
        );
        assert!(
            matches!(read(&mut reader, &mut fragments).await.unwrap(), Message::Text(v) if v==br#"{"a":0}"#)
        );
        let mut encoded = Vec::new();
        write(&mut encoded, 1, b"hello").await.unwrap();
        assert_eq!(&encoded[..2], &[129, 133]);
        let decoded: Vec<_> =
            encoded[6..].iter().enumerate().map(|(i, b)| b ^ encoded[2 + i % 4]).collect();
        assert_eq!(decoded, b"hello");
    }
    #[tokio::test]
    async fn hostile_headers_are_rejected_before_reading_or_allocating_payload() {
        for bytes in [
            vec![129, 128],
            vec![193, 0],
            vec![130, 0],
            vec![128, 0],
            vec![9, 0],
            vec![129, 126, 0, 1],
            vec![129, 1, 255],
            vec![136, 1, 0],
        ] {
            assert_eq!(
                read(&mut bytes.as_slice(), &mut None).await.unwrap_err(),
                LinkError::Protocol
            );
        }
        let mut huge = vec![129, 127];
        huge.extend_from_slice(&((MAX as u64) + 1).to_be_bytes());
        assert_eq!(read(&mut huge.as_slice(), &mut None).await.unwrap_err(), LinkError::TooLarge);
    }
}
