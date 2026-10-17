mod cache;
mod config;
mod discovery;
mod heartbeat;
mod https;
mod process;
mod schema;
mod watch;

use config::Config;
use discovery::Catalogue;
use process::Jobs;
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::mpsc,
    thread,
    time::Duration,
};
const VERSIONS: [&str; 3] = ["2024-11-05", "2025-03-26", "2025-06-18"];
struct Server {
    config: Config,
    catalogue: Option<Catalogue>,
    jobs: Jobs,
    initialized: bool,
    heartbeat: heartbeat::Heartbeat,
}
fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
impl Server {
    fn new(config: Config) -> Self {
        Self {
            config,
            catalogue: None,
            jobs: Jobs::default(),
            initialized: false,
            heartbeat: heartbeat::Heartbeat::start(),
        }
    }
    fn tick(&mut self) {
        let _ = self.jobs.manage(&json!({"action":"list"}));
    }
    fn request(&mut self, line: &[u8]) -> Vec<Value> {
        let value: Value = match serde_json::from_slice(line) {
            Ok(v) => v,
            Err(_) => return vec![error(Value::Null, -32700, "Parse error")],
        };
        let id = value.get("id").cloned().unwrap_or(Value::Null);
        if !value.is_object()
            || value["jsonrpc"] != "2.0"
            || !value["method"].is_string()
            || !(id.is_null() || id.is_number() || id.is_string())
        {
            return vec![error(Value::Null, -32600, "Invalid Request")];
        }
        let method = value["method"].as_str().unwrap_or_default();
        if value.get("id").is_none() {
            if method == "notifications/initialized" {
                self.initialized = true;
            }
            return vec![];
        }
        let params = value.get("params").cloned().unwrap_or_else(|| json!({}));
        if !params.is_object() {
            return vec![error(id, -32602, "Params must be an object")];
        }
        let mut changed = false;
        let result = match method {
            "initialize" => {
                let Some(version) = params["protocolVersion"].as_str() else {
                    return vec![error(id, -32602, "protocolVersion is required")];
                };
                let version = if VERSIONS.contains(&version) { version } else { VERSIONS[2] };
                let catalogue = Catalogue::discover(&self.config);
                let response = json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": true}},
                    "serverInfo": {"name": "moonglow-mcp", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": catalogue.instructions
                });
                self.heartbeat.sessions(catalogue.heartbeat_sessions());
                self.catalogue = Some(catalogue);
                response
            }
            "ping" => json!({}),
            "tools/list" => {
                let catalogue = Catalogue::discover(&self.config);
                changed = self.initialized
                    && self.catalogue.as_ref().is_some_and(|old| {
                        serde_json::to_value(&old.tools).ok()
                            != serde_json::to_value(&catalogue.tools).ok()
                    });
                let response = json!({"tools":catalogue.tools});
                self.heartbeat.sessions(catalogue.heartbeat_sessions());
                self.catalogue = Some(catalogue);
                response
            }
            "tools/call" => {
                let Some(name) = params["name"].as_str() else {
                    return vec![error(id, -32602, "Tool name is required")];
                };
                let input = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                if !input.is_object() {
                    return vec![error(id, -32602, "Tool arguments must be an object")];
                }
                let result = self
                    .catalogue
                    .as_mut()
                    .ok_or("Initialize or list tools first".into())
                    .and_then(|c| c.call(name, &input, &self.config, &mut self.jobs));
                let (value, failed) = match result {
                    Ok(v) => {
                        let failed = v.get("exitCode").is_some() && v["ok"] == false;
                        (v, failed)
                    }
                    Err(e) => (json!({"error":e}), true),
                };
                // structuredContent is always an object, including array/scalar operation results.
                let structured =
                    if value.is_object() { value.clone() } else { json!({"result":value}) };
                json!({
                    "content": [{"type": "text", "text": value.to_string()}],
                    "structuredContent": structured,
                    "isError": failed
                })
            }
            _ => return vec![error(id, -32601, "Method not found")],
        };
        let mut responses = vec![json!({"jsonrpc":"2.0","id":id,"result":result})];
        if changed {
            responses.push(json!({"jsonrpc":"2.0","method":"notifications/tools/list_changed"}));
        }
        responses
    }
}
fn stdio(mut server: Server) -> Result<(), String> {
    let (tx, rx) = mpsc::sync_channel(4);
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        loop {
            let mut line = Vec::new();
            let mut oversized = false;
            loop {
                let bytes = match input.fill_buf() {
                    Ok(b) => b,
                    Err(_) => return,
                };
                if bytes.is_empty() {
                    break;
                }
                let n = bytes.iter().position(|b| *b == b'\n').map_or(bytes.len(), |i| i + 1);
                let done = bytes[n - 1] == b'\n';
                if line.len() + n > https::LIMIT {
                    oversized = true;
                } else if !oversized {
                    line.extend_from_slice(&bytes[..n]);
                }
                input.consume(n);
                if done {
                    break;
                }
            }
            if oversized {
                line = b"{".to_vec();
            }
            if line.is_empty() {
                break;
            }
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let stdout = io::stdout();
    let mut output = stdout.lock();
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                for response in server.request(&line) {
                    writeln!(output, "{response}").map_err(|e| e.to_string())?;
                }
                output.flush().map_err(|e| e.to_string())?;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        server.tick();
    }
    Ok(())
}
fn send_http(stream: &mut TcpStream, status: &str, body: &str) -> Result<(), String> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())
}
fn http_request(server: &mut Server, stream: &mut TcpStream, token: &str) -> Result<(), String> {
    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    let (headers, body) = match https::read_message(stream) {
        Ok(v) => v,
        Err(_) => return send_http(stream, "400 Bad Request", "{}"),
    };
    let mut auth = None;
    let mut version = None;
    for line in headers.lines().skip(1) {
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("authorization") {
                if auth.is_some() {
                    return send_http(stream, "400 Bad Request", "{}");
                }
                auth = Some(value.trim());
            }
            if key.eq_ignore_ascii_case("mcp-protocol-version") {
                version = Some(value.trim());
            }
        }
    }
    if auth != Some(format!("Bearer {token}").as_str()) {
        return send_http(stream, "401 Unauthorized", "{}");
    }
    if version.is_some_and(|v| !VERSIONS.contains(&v)) {
        return send_http(stream, "400 Bad Request", "{}");
    }
    let mut first = headers.lines().next().unwrap_or_default().split_whitespace();
    if first.next() != Some("POST") {
        return send_http(stream, "405 Method Not Allowed", "{}");
    }
    if first.next() != Some("/mcp") {
        return send_http(stream, "404 Not Found", "{}");
    }
    let responses = server.request(&body);
    // Stateless HTTP has no unsolicited notification stream: clients re-list to refresh.
    if let Some(response) = responses.iter().find(|v| v.get("id").is_some()) {
        send_http(stream, "200 OK", &response.to_string())
    } else {
        send_http(stream, "202 Accepted", "")
    }
}
fn http(mut server: Server) -> Result<(), String> {
    let address: SocketAddr = server
        .config
        .http
        .as_ref()
        .ok_or("Missing HTTP address")?
        .parse()
        .map_err(|e| format!("HTTP needs an IP address and port: {e}"))?;
    if !server.config.remote && !address.ip().is_loopback() {
        return Err("Non-loopback HTTP requires --http-allow-remote and TLS termination".into());
    }
    let token = server.config.http_token.clone().ok_or("Missing HTTP token")?;
    let listener = TcpListener::bind(address).map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    eprintln!("MCP HTTP listening on {}", listener.local_addr().map_err(|e| e.to_string())?);
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if let Err(e) = http_request(&mut server, &mut stream, &token) {
                    eprintln!("HTTP request failed: {e}");
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20))
            }
            Err(e) => return Err(e.to_string()),
        }
        server.tick();
    }
}
/// From a shell, for agents without MCP: `tools` lists the same tools an MCP
/// client sees; `call TOOL [JSON]` calls one and prints its result (exit 1 on
/// error).
fn cli(mut server: Server, args: &[String]) -> Result<(), String> {
    let usage = watch::USAGE;
    match args {
        [command] if command == "help" => {
            println!("{usage}");
            Ok(())
        }
        [command] if command == "tools" => {
            let listed = server.request(br#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
            let tools = listed[0]["result"]["tools"].as_array().cloned().unwrap_or_default();
            for t in &tools {
                let about = t["description"].as_str().unwrap_or_default();
                let name = t["name"].as_str().unwrap_or_default();
                println!("{name}	{}", about.lines().next().unwrap_or(""));
            }
            Ok(())
        }
        [command, name, rest @ ..] if command == "call" => {
            let input: Value = match rest {
                [] => json!({}),
                [text] => {
                    serde_json::from_str(text).map_err(|e| format!("Invalid JSON input: {e}"))?
                }
                _ => return Err(usage.into()),
            };
            server.catalogue = Some(Catalogue::for_call(&server.config, name));
            let request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
                "params":{"name":name,"arguments":input}});
            let answer = server.request(request.to_string().as_bytes());
            if let Some(message) = answer[0]["error"]["message"].as_str() {
                return Err(message.into());
            }
            let result = &answer[0]["result"];
            let shown = serde_json::to_string_pretty(&result["structuredContent"])
                .map_err(|e| e.to_string())?;
            println!("{shown}");
            if result["isError"] == true { Err(format!("{name} failed")) } else { Ok(()) }
        }
        _ => Err(usage.into()),
    }
}

fn main() {
    let result = Config::parse().and_then(|c| {
        let is_http = c.http.is_some();
        let one = c.cli.clone();
        if one.first().is_some_and(|s| s == "watch") {
            return watch::run(c, &one[1..]);
        }
        let server = Server::new(c);
        if !one.is_empty() {
            cli(server, &one)
        } else if is_http {
            http(server)
        } else {
            stdio(server)
        }
    });
    if let Err(e) = result {
        eprintln!("moonglow-mcp: {e}");
        std::process::exit(1);
    }
}
