mod support;

use std::net::TcpStream;
use std::time::Duration;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use mg_live_link::{Error, Gateway, Login};
use sha2::{Digest, Sha256};
use support::{FakeGateway, callback};

#[test]
fn garbage_connection_then_real_callback_succeeds() {
    use std::io::{Read, Write};
    let fake = FakeGateway::start();
    let login =
        Login::begin(Gateway::new(&fake.url, Some(&fake.pin)).unwrap(), Duration::from_secs(5))
            .unwrap();
    let url = login.browser_url().to_owned();
    let start = url::Url::parse(&url).unwrap();
    let port: u16 = start.query_pairs().find(|(k, _)| k == "port").unwrap().1.parse().unwrap();
    let worker = std::thread::spawn(move || login.finish());
    let mut probe = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).unwrap();
    probe.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    probe.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    let mut response = String::new();
    probe.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 400"));
    let (_, response) = callback(&url, true);
    assert!(response.starts_with("HTTP/1.1 400"));
    let (_, response) = callback(&url, false);
    assert!(response.starts_with("HTTP/1.1 200"));
    assert_eq!(worker.join().unwrap().unwrap().expose(), "fixture-session");
    fake.finish();
}

#[test]
fn idle_probe_cannot_consume_the_callback_deadline() {
    let fake = FakeGateway::start();
    let login =
        Login::begin(Gateway::new(&fake.url, Some(&fake.pin)).unwrap(), Duration::from_secs(5))
            .unwrap();
    let url = login.browser_url().to_owned();
    let start = url::Url::parse(&url).unwrap();
    let port: u16 = start.query_pairs().find(|(k, _)| k == "port").unwrap().1.parse().unwrap();
    let idle = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).unwrap();
    let worker = std::thread::spawn(move || login.finish());
    let (_, response) = callback(&url, false);
    assert!(response.starts_with("HTTP/1.1 200"));
    assert_eq!(worker.join().unwrap().unwrap().expose(), "fixture-session");
    drop(idle);
    fake.finish();
}

#[test]
fn callback_attempts_are_capped() {
    let login =
        Login::begin(Gateway::new("https://localhost:1", None).unwrap(), Duration::from_secs(10))
            .unwrap();
    let url = login.browser_url().to_owned();
    let worker = std::thread::spawn(move || login.finish());
    let mut port = 0;
    for _ in 0..32 {
        let (callback_port, response) = callback(&url, true);
        port = callback_port;
        assert!(response.starts_with("HTTP/1.1 400"));
    }
    assert!(matches!(worker.join().unwrap(), Err(Error::Callback)));
    assert!(TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).is_err());
}

#[test]
fn happy_path_pkce_headers_and_second_callback_refused() {
    let fake = FakeGateway::start();
    let login =
        Login::begin(Gateway::new(&fake.url, Some(&fake.pin)).unwrap(), Duration::from_secs(5))
            .unwrap();
    let url = login.browser_url().to_owned();
    let worker = std::thread::spawn(move || login.finish());
    let (port, response) = callback(&url, false);
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("Referrer-Policy: no-referrer\r\n"));
    assert!(response.contains("default-src 'none'"));
    assert!(!response.contains("fixture-grant"));
    assert!(TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).is_err());
    let token = worker.join().unwrap().unwrap();
    assert_eq!(token.expose(), "fixture-session");
    assert!(!format!("{token:?}").contains(token.expose()));
    let request = fake.request.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(request["grant"], "fixture-grant");
    let verifier = request["verifier"].as_str().unwrap();
    assert_eq!(verifier.len(), 43);
    let start = url::Url::parse(&url).unwrap();
    let challenge = start.query_pairs().find(|(k, _)| k == "challenge").unwrap().1.into_owned();
    assert_eq!(challenge, URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())));
    fake.finish();
}

#[test]
fn wrong_state_is_rejected_and_shared_deadline_closes_listener() {
    let login =
        Login::begin(Gateway::new("https://localhost:1", None).unwrap(), Duration::from_secs(2))
            .unwrap();
    let url = login.browser_url().to_owned();
    let worker = std::thread::spawn(move || login.finish());
    let (port, response) = callback(&url, true);
    assert!(response.starts_with("HTTP/1.1 400"));
    assert!(matches!(worker.join().unwrap(), Err(Error::Timeout)));
    assert!(TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).is_err());
}

#[test]
fn timeout_closes_listener() {
    let login =
        Login::begin(Gateway::new("https://localhost:1", None).unwrap(), Duration::from_millis(30))
            .unwrap();
    let url = url::Url::parse(login.browser_url()).unwrap();
    let port: u16 = url.query_pairs().find(|(k, _)| k == "port").unwrap().1.parse().unwrap();
    assert!(matches!(login.finish(), Err(Error::Timeout)));
    assert!(TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).is_err());
}

#[test]
fn pin_mismatch_never_posts_a_grant() {
    let fake = FakeGateway::start();
    let login = Login::begin(
        Gateway::new(&fake.url, Some(&"0".repeat(64))).unwrap(),
        Duration::from_secs(3),
    )
    .unwrap();
    let url = login.browser_url().to_owned();
    let worker = std::thread::spawn(move || login.finish());
    callback(&url, false);
    assert!(matches!(worker.join().unwrap(), Err(Error::PinMismatch)));
    assert!(fake.request.recv_timeout(Duration::from_millis(100)).is_err());
    fake.finish();
}

#[test]
fn tofu_returns_fingerprint_without_http_or_implicit_trust() {
    let fake = FakeGateway::start();
    let error =
        Login::begin(Gateway::new(&fake.url, None).unwrap(), Duration::from_secs(3)).unwrap_err();
    let Error::TrustRequired(prompt) = error else { panic!("missing trust decision") };
    assert_eq!(prompt.sha256, fake.pin);
    assert_eq!(prompt.origin, fake.url);
    assert!(fake.request.recv_timeout(Duration::from_millis(100)).is_err());
    fake.finish();
}

#[test]
fn reject_unsafe_urls_and_bad_pins() {
    for url in [
        "http://example.org",
        "ws://example.org",
        "http://192.0.2.1:5199",
        "https://user:secret@example.org",
        "https://example.org/path",
        "https://example.org?query",
        "https://example.org#fragment",
    ] {
        assert!(matches!(Gateway::new(url, None), Err(Error::InvalidGateway)));
    }
    assert!(matches!(Gateway::new("https://example.org", Some("abc")), Err(Error::InvalidPin)));
    assert!(Gateway::new("wss://example.org", None).is_ok());
    // A development gateway on this machine may speak plain HTTP.
    for url in ["http://127.0.0.1:5199", "ws://[::1]:5199", "http://localhost:5199"] {
        let gateway = Gateway::new(url, None).unwrap();
        // No TLS probe and no trust prompt: nothing listens there, so any I/O would fail.
        gateway.check_trust(std::time::Duration::from_millis(50)).unwrap();
        assert!(gateway.tls_config().is_ok(), "{url}");
    }
    let pin = "a".repeat(64);
    assert!(matches!(Gateway::new("http://127.0.0.1:5199", Some(&pin)), Err(Error::InvalidPin)));
}

#[test]
fn slow_certificate_discovery_cannot_extend_the_deadline() {
    use std::io::Write;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("https://{}", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket.set_write_timeout(Some(Duration::from_millis(100))).unwrap();
        // An incomplete TLS handshake record, trickling faster than the socket timeout.
        if socket.write_all(&[22, 3, 3, 0, 100]).is_err() {
            return;
        }
        for _ in 0..30 {
            if socket.write_all(&[0]).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    let began = std::time::Instant::now();
    let result = Gateway::new(&url, None).unwrap().check_trust(Duration::from_millis(80));
    assert!(matches!(result, Err(Error::Timeout)));
    assert!(began.elapsed() < Duration::from_millis(400));
    worker.join().unwrap();
}
