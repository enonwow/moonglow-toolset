use std::{env, path::PathBuf};

#[derive(Clone)]
pub struct Config {
    pub gateway: Option<String>,
    pub gateway_pin: Option<String>,
    pub ai_token: Option<String>,
    pub session_id: String,
    pub mg: PathBuf,
    pub roots: Vec<PathBuf>,
    pub write: bool,
    pub http: Option<String>,
    pub http_token: Option<String>,
    pub remote: bool,
    /// Shell discovery/calls or a streaming event watch instead of an MCP server.
    pub cli: Vec<String>,
}
pub fn data_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        env::var_os("APPDATA").map(|p| PathBuf::from(p).join("Moonglow"))
    } else if cfg!(target_os = "macos") {
        env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support/Moonglow"))
    } else {
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
            .map(|p| p.join("moonglow"))
    }
}
impl Config {
    pub fn parse() -> Result<Self, String> {
        let sibling = env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join(if cfg!(windows) { "mg.exe" } else { "mg" })))
            .filter(|p| p.is_file());
        let mut c = Self {
            gateway: None,
            gateway_pin: None,
            ai_token: env::var("MOONGLOW_AI_TOKEN").ok(),
            session_id: "gateway".into(),
            mg: env::var_os("MOONGLOW_MG")
                .map(PathBuf::from)
                .or(sibling)
                .unwrap_or_else(|| "mg".into()),
            roots: env::var_os("MOONGLOW_MCP_ROOTS")
                .map(|v| env::split_paths(&v).collect())
                .unwrap_or_default(),
            write: env::var("MOONGLOW_MCP_WRITE").is_ok_and(|v| v == "1"),
            http: None,
            http_token: env::var("MOONGLOW_MCP_HTTP_TOKEN").ok(),
            remote: false,
            cli: Vec::new(),
        };
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => c.cli = vec!["help".into()],
                "--allow-write" => c.write = true,
                "--http-allow-remote" => c.remote = true,
                "--mg" | "--roots" | "--http" | "--http-token" | "--gateway" | "--gateway-pin"
                | "--session-id" => {
                    let value = args.next().ok_or_else(|| format!("Missing value for {arg}"))?;
                    match arg.as_str() {
                        "--gateway" => c.gateway = Some(value),
                        "--gateway-pin" => c.gateway_pin = Some(value),
                        "--session-id" => c.session_id = value,
                        "--mg" => c.mg = value.into(),
                        "--roots" => c.roots.push(value.into()),
                        "--http" => c.http = Some(value),
                        _ => c.http_token = Some(value),
                    }
                }
                "--session" | "--since" | "--from" | "--kinds" | "--area" | "--count"
                    if c.cli.first().is_some_and(|s| s == "watch") =>
                {
                    let value = args.next().ok_or_else(|| format!("Missing value for {arg}"))?;
                    c.cli.extend([arg, value]);
                }
                "--to-dm" if c.cli.first().is_some_and(|s| s == "watch") => c.cli.push(arg),
                _ if !arg.starts_with('-') => c.cli.push(arg),
                _ => {
                    return Err(format!(
                        "Unknown option {arg}. Usage: moonglow-mcp [tools | call TOOL [JSON]] --gateway URL [--gateway-pin SHA256] --mg PROGRAM \
                         --roots DIR (repeatable) --allow-write --http ADDRESS \
                         --http-token TOKEN --http-allow-remote"
                    ));
                }
            }
        }
        if let Some(url) = &c.gateway {
            mg_live_link::Gateway::new(url, c.gateway_pin.as_deref()).map_err(|e| e.to_string())?;
            if !c.ai_token.as_ref().is_some_and(|s| {
                !s.is_empty() && s.len() <= 4096 && s.bytes().all(|b| b.is_ascii_graphic())
            }) {
                return Err("Gateway requires MOONGLOW_AI_TOKEN in the environment".into());
            }
            if c.session_id.is_empty()
                || c.session_id.len() > 128
                || c.session_id.chars().any(char::is_control)
            {
                return Err("Invalid gateway session ID".into());
            }
        } else if c.gateway_pin.is_some() {
            return Err("--gateway-pin requires --gateway".into());
        }
        if c.roots.is_empty() {
            c.roots.push(env::current_dir().map_err(|e| e.to_string())?);
        }
        c.roots = c
            .roots
            .iter()
            .map(|p| p.canonicalize().map_err(|e| format!("Invalid root: {e}")))
            .collect::<Result<_, _>>()?;
        if c.roots.iter().any(|p| !p.is_dir()) {
            return Err("Roots must be directories".into());
        }
        if c.http.is_some()
            && !c
                .http_token
                .as_ref()
                .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_graphic()))
        {
            return Err(
                "HTTP requires a nonempty ASCII --http-token (or MOONGLOW_MCP_HTTP_TOKEN)".into()
            );
        }
        Ok(c)
    }
}
