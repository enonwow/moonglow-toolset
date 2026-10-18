#[path = "link_support/mod.rs"]
#[allow(dead_code)]
mod support;

use mg_live_link::Gateway;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exported_tls_requires_ip_trust_and_checks_pins_and_public_roots() {
    for origin in ["https://127.0.0.1:1", "https://[::1]:1"] {
        assert!(Gateway::new(origin, None).unwrap().tls_config().is_err());
    }
    let host = support::Host::new().await;
    let address = host.origin.trim_start_matches("https://");
    for (pin, accepted) in
        [(Some(host.pin.as_str()), true), (Some("00".repeat(32).as_str()), false), (None, false)]
    {
        // A self-signed certificate succeeds only with the exact saved pin;
        // a DNS origin without a pin still requires public WebPKI trust.
        let gateway = Gateway::new("https://localhost", pin).unwrap();
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let connector = tokio_rustls::TlsConnector::from(gateway.tls_config().unwrap());
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            connector
                .connect(rustls::pki_types::ServerName::try_from("localhost").unwrap(), stream),
        )
        .await
        .unwrap();
        assert_eq!(result.is_ok(), accepted);
    }
    assert!(Gateway::new(&host.origin, Some(&host.pin)).unwrap().tls_config().is_ok());
}
