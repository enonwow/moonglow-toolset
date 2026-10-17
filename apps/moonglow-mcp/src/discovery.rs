use crate::{
    cache,
    config::{Config, data_dir},
    https,
    process::{Jobs, Running, arguments},
    schema,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[cfg(not(windows))]
use std::process::Stdio;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};

/// What every client is told at `initialize`; the agents' own instructions
/// and the discovery diagnostics are appended.
const INSTRUCTIONS: &str = "Tools and their semantics are discovered from the connected agents \
    and CLI. Follow each tool's description. Agent policies hide disabled operations. CLI path \
    arguments stay within the configured roots; writes require the adapter's write switch. \
    Long-running commands require background:true; manage only this adapter's jobs with \
    moonglow_jobs.";

#[derive(Clone, Deserialize)]
pub struct Descriptor {
    protocol: String,
    url: String,
    token: String,
    #[serde(rename = "certSha256")]
    pin: String,
    pub pid: u32,
    #[serde(skip)]
    client: Arc<Mutex<https::Client>>,
    #[serde(skip)]
    key: String,
    #[serde(skip)]
    fingerprint: String,
    #[serde(skip)]
    revision: Option<String>,
}
impl Descriptor {
    pub(crate) fn ping(&self) {
        let _ = https::request_timeout(
            &self.url,
            &self.token,
            &self.pin,
            "/v1/ping",
            None,
            std::time::Duration::from_secs(2),
        );
    }
    fn response(
        &self,
        path: &str,
        body: Option<&Value>,
        revision: Option<&str>,
    ) -> Result<https::Reply, String> {
        self.client.lock().map_err(|_| "Agent connection lock failed")?.request(
            (&self.url, &self.token, &self.pin),
            path,
            body,
            revision,
            Duration::from_secs(120),
        )
    }
    fn describe(&mut self, cached: bool) -> Result<Value, String> {
        if cached
            && let Some((describe, etag)) = cache::load(&self.key, &self.fingerprint)
            && describe["protocol"] == "moonglow-agent/1"
            && describe["operations"].is_array()
        {
            self.revision = Some(etag);
            return Ok(describe);
        }
        let reply = self.response("/v1/describe", None, None)?;
        self.revision = reply.etag.clone();
        let value = reply.json()?;
        if let Some(etag) = &self.revision {
            cache::save(&self.key, &self.fingerprint, &value, etag);
        }
        Ok(value)
    }
}
#[derive(Clone)]
pub struct Session {
    descriptor: Descriptor,
    describe: Value,
}
#[derive(Clone)]
enum Target {
    Sessions,
    Jobs,
    Agent(Vec<Descriptor>),
    Cli(Value),
}
#[derive(Clone, Serialize)]
pub struct Tool {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input: Value,
    annotations: Value,
    #[serde(skip)]
    target: Target,
}
pub struct Catalogue {
    pub tools: Vec<Tool>,
    pub instructions: String,
    pub diagnostics: Vec<String>,
    sessions: Vec<Session>,
}
fn alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    if pid == std::process::id() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .creation_flags(0x08000000)
            .output()
            .ok()
            .is_some_and(|o| {
                String::from_utf8_lossy(&o.stdout).lines().any(|s| {
                    s.split(',').nth(1).is_some_and(|s| s.trim_matches('"') == pid.to_string())
                })
            })
    }
    #[cfg(not(windows))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
}
fn read_descriptor(path: &Path) -> Result<Descriptor, String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 65536 {
        return Err("Descriptor too large".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let mut d: Descriptor =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid descriptor JSON")?;
    if d.protocol != "moonglow-agent/1" {
        return Err("Unsupported agent protocol".into());
    }
    d.key =
        cache::hash(path.canonicalize().map_err(|e| e.to_string())?.to_string_lossy().as_bytes());
    d.fingerprint = cache::hash(&bytes);
    Ok(d)
}
fn tool(
    name: String,
    description: String,
    input: Value,
    read: bool,
    destructive: bool,
    target: Target,
) -> Result<Tool, String> {
    if !schema::name(&name) || input["type"] != "object" {
        return Err("Invalid operation name or input object".into());
    }
    schema::check(&input, false)?;
    Ok(Tool {
        name,
        description,
        input,
        annotations: json!({
            "readOnlyHint": read,
            "destructiveHint": destructive,
            "openWorldHint": true
        }),
        target,
    })
}
impl Catalogue {
    pub fn heartbeat_sessions(&self) -> Vec<Descriptor> {
        self.sessions.iter().map(|s| s.descriptor.clone()).collect()
    }
    pub fn discover(config: &Config) -> Self {
        Self::load(config, false, true)
    }
    /// Shell calls only need agent descriptions; CLI discovery is a fallback for CLI tools.
    pub fn for_call(config: &Config, name: &str) -> Self {
        let cached = Self::load(config, true, false);
        if cached.tools.iter().any(|t| t.name == name && matches!(t.target, Target::Agent(_))) {
            cached
        } else {
            Self::discover(config)
        }
    }
    pub fn for_watch(
        config: &Config,
        pid: Option<u32>,
    ) -> Result<(Self, String, Option<String>), String> {
        let mut catalogue = Self::load(config, true, false);
        // A newly upgraded agent may advertise the streaming capability after our cache was saved.
        if catalogue.event_stream(pid).is_err() {
            catalogue = Self::load(config, false, false);
        }
        let (name, session) = catalogue.event_stream(pid)?;
        Ok((catalogue, name, session))
    }
    fn event_stream(&self, pid: Option<u32>) -> Result<(String, Option<String>), String> {
        let session = match pid {
            Some(pid) => self
                .sessions
                .iter()
                .find(|s| s.descriptor.pid == pid)
                .ok_or("Requested watch session is unavailable")?,
            None if self.sessions.len() == 1 => &self.sessions[0],
            None => {
                return Err(format!(
                    "Watch needs one session; use --session PID ({} found). {}",
                    self.sessions.len(),
                    self.diagnostics.join("; ")
                ));
            }
        };
        let name = session.describe["capabilities"]["eventStream"]["operation"].as_str().ok_or(
            "Agent does not advertise eventStream; update it or use its discovered event operation",
        )?;
        if !self.tools.iter().any(|t| t.name == name && t.annotations["readOnlyHint"] == true) {
            return Err("Advertised event stream is unavailable or is not read-only".into());
        }
        Ok((name.into(), (self.sessions.len() > 1).then(|| session.descriptor.pid.to_string())))
    }
    fn refresh_agents(&mut self, config: &Config) {
        let cli: Vec<_> =
            self.tools.iter().filter(|t| matches!(t.target, Target::Cli(_))).cloned().collect();
        *self = Self::load(config, false, false);
        for tool in cli {
            self.add(Ok(tool));
        }
    }
    fn load(config: &Config, cached: bool, include_cli: bool) -> Self {
        let mut result = Self {
            tools: Vec::new(),
            instructions: INSTRUCTIONS.into(),
            diagnostics: Vec::new(),
            sessions: Vec::new(),
        };
        let mut paths = config.descriptors.clone();
        if paths.is_empty() {
            let own = data_dir().map(|p| p.join("live/sessions"));
            for dir in own.iter().chain(&config.sessions) {
                let Ok(entries) = fs::read_dir(dir) else { continue };
                paths.extend(
                    entries
                        .filter_map(Result::ok)
                        .map(|e| e.path())
                        .filter(|p| p.extension().is_some_and(|e| e == "json")),
                );
            }
        }
        paths.sort();
        paths.dedup();
        for path in paths {
            let mut d = match read_descriptor(&path) {
                Ok(d) => d,
                Err(e) => {
                    result.diagnostics.push(format!("Descriptor unavailable: {e}"));
                    continue;
                }
            };
            // Explicit descriptors may represent a tunnel whose PID is not local.
            if !cached && include_cli && config.descriptors.is_empty() && !alive(d.pid) {
                continue;
            }
            if result.sessions.iter().any(|s| s.descriptor.pid == d.pid) {
                result.diagnostics.push("Duplicate session PID ignored".into());
                continue;
            }
            match d.describe(cached) {
                Ok(value)
                    if value["protocol"] == "moonglow-agent/1"
                        && value["operations"].is_array() =>
                {
                    if let Some(instructions) = value["instructions"].as_str() {
                        result.instructions.push('\n');
                        result.instructions.push_str(instructions);
                    }
                    result.sessions.push(Session { descriptor: d, describe: value });
                }
                Ok(_) => result.diagnostics.push("Agent did not describe moonglow-agent/1".into()),
                Err(e) => result.diagnostics.push(format!("Agent discovery failed: {e}")),
            }
        }
        let sessions_input = json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        });
        let sessions = tool(
            "moonglow_sessions".into(),
            "List discovered agent sessions, versions, variants and discovery diagnostics. \
             Credentials are never returned."
                .into(),
            sessions_input,
            true,
            false,
            Target::Sessions,
        );
        result.tools.push(sessions.expect("static schema"));
        let jobs_input = json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "status", "stop", "forget"],
                    "default": "list",
                    "description": "Job management action"
                },
                "job": {"type": "string", "description": "Owned job ID; required except for list"}
            },
            "additionalProperties": false
        });
        let jobs = tool(
            "moonglow_jobs".into(),
            "List, inspect, stop or forget CLI jobs started by this adapter. \
             Job IDs have meaning only in this adapter process."
                .into(),
            jobs_input,
            false,
            true,
            Target::Jobs,
        );
        result.tools.push(jobs.expect("static schema"));
        let mut candidates: BTreeMap<String, Vec<(Value, Descriptor)>> = BTreeMap::new();
        for session in &result.sessions {
            for op in session.describe["operations"].as_array().into_iter().flatten() {
                if let Some(name) = op["name"].as_str() {
                    candidates
                        .entry(name.into())
                        .or_default()
                        .push((op.clone(), session.descriptor.clone()));
                }
            }
        }
        for (name, ops) in candidates {
            let op = &ops[0].0;
            if op["input"]["type"] != "object" || schema::check(&op["input"], false).is_err() {
                result.diagnostics.push(format!("Invalid input schema for {name}"));
                continue;
            }
            if ops.iter().any(|(other, _)| {
                other["input"] != op["input"]
                    || other["readOnly"] != op["readOnly"]
                    || other["destructive"] != op["destructive"]
            }) {
                result
                    .diagnostics
                    .push(format!("Incompatible descriptions for {name}; operation omitted"));
                continue;
            }
            let mut input = op["input"].clone();
            if result.sessions.len() > 1 {
                if input["properties"].get("session").is_some() {
                    result
                        .diagnostics
                        .push(format!("Reserved session argument in {name}; operation omitted"));
                    continue;
                }
                if let Some(props) = input["properties"].as_object_mut() {
                    let pids: Vec<String> = ops.iter().map(|(_, d)| d.pid.to_string()).collect();
                    props.insert(
                        "session".into(),
                        json!({"type": "string", "description": "Agent session PID", "enum": pids}),
                    );
                }
                if input.get("required").is_none() {
                    input["required"] = json!([]);
                }
                if let Some(req) = input["required"].as_array_mut() {
                    req.push(json!("session"));
                }
            }
            let created = tool(
                name,
                op["description"].as_str().unwrap_or("Discovered agent operation").into(),
                input,
                op["readOnly"] == true,
                op["destructive"] != false,
                Target::Agent(ops.iter().map(|(_, d)| d.clone()).collect()),
            );
            result.add(created);
        }
        if include_cli {
            let cli = Running::start(&config.mg, &["describe".into(), "--json".into()], 5000)
                .and_then(Running::wait);
            match cli {
                Ok(output)
                    if output["ok"] == true
                        && output["stdout"]["protocol"] == "moonglow-cli/1"
                        && output["stdout"]["commands"].is_array() =>
                {
                    for command in output["stdout"]["commands"].as_array().into_iter().flatten() {
                        result.add_cli(command);
                    }
                }
                _ => result.diagnostics.push(
                    "CLI discovery unavailable: mg describe did not return moonglow-cli/1; \
                 no mg_* tools are offered."
                        .into(),
                ),
            }
        }
        if result.sessions.is_empty()
            && !result.tools.iter().any(|t| matches!(t.target, Target::Cli(_)))
        {
            result.diagnostics.push(
                "No compatible agent or CLI source found. \
                 Configure --descriptor or --mg, or start an agent."
                    .into(),
            );
        }
        for d in &result.diagnostics {
            result.instructions.push('\n');
            result.instructions.push_str(d);
        }
        result
    }
    /// One `mg describe` command as an `mg_*` tool, with the adapter's own
    /// `timeout_ms` and `background` arguments added.
    fn add_cli(&mut self, command: &Value) {
        let Some(name) = command["name"].as_str() else { return };
        if !command["writes"].is_boolean()
            || !command["longRunning"].is_boolean()
            || !command["argv"].is_array()
            || !command["path"].is_array()
        {
            self.diagnostics.push("Incomplete CLI description ignored".into());
            return;
        }
        if command["input"]["type"] != "object" || schema::check(&command["input"], false).is_err()
        {
            self.diagnostics.push(format!("Invalid CLI input schema for {name}"));
            return;
        }
        let mut input = command["input"].clone();
        if input["properties"].get("timeout_ms").is_some()
            || input["properties"].get("background").is_some()
        {
            self.diagnostics
                .push(format!("Reserved adapter argument in CLI {name}; command omitted"));
            return;
        }
        if let Some(props) = input["properties"].as_object_mut() {
            props.insert(
                "timeout_ms".into(),
                json!({
                    "type": "integer",
                    "description": "Process timeout in milliseconds",
                    "minimum": 1,
                    "maximum": 1800000,
                    "default": 120000
                }),
            );
            props.insert(
                "background".into(),
                json!({
                    "type": "boolean",
                    "description":
                        "Return an owned job ID immediately; mandatory for long-running commands",
                    "default": false
                }),
            );
        }
        let created = tool(
            format!("mg_{name}"),
            command["description"].as_str().unwrap_or("Discovered CLI command").into(),
            input,
            command["writes"] == false,
            command["writes"] == true,
            Target::Cli(command.clone()),
        );
        self.add(created);
    }
    fn add(&mut self, created: Result<Tool, String>) {
        match created {
            Ok(t) if !self.tools.iter().any(|old| old.name == t.name) => self.tools.push(t),
            Ok(t) => self.diagnostics.push(format!("Duplicate tool {} omitted", t.name)),
            Err(e) => self.diagnostics.push(format!("Invalid operation: {e}")),
        }
    }
    pub fn call(
        &mut self,
        name: &str,
        input: &Value,
        config: &Config,
        jobs: &mut Jobs,
    ) -> Result<Value, String> {
        match self.call_once(name, input, config, jobs) {
            Ok(value) => Ok(value),
            Err(CallFailure::Other(error)) => Err(error),
            Err(CallFailure::Refresh(_)) => {
                self.refresh_agents(config);
                self.call_once(name, input, config, jobs).map_err(|e| match e {
                    CallFailure::Other(e) | CallFailure::Refresh(e) => e,
                })
            }
        }
    }
    fn call_once(
        &self,
        name: &str,
        input: &Value,
        config: &Config,
        jobs: &mut Jobs,
    ) -> Result<Value, CallFailure> {
        let tool =
            self.tools.iter().find(|t| t.name == name).ok_or("Unknown tool; refresh tools/list")?;
        schema::validate(&tool.input, input).map_err(|e| {
            if matches!(tool.target, Target::Agent(_)) {
                CallFailure::Refresh(e)
            } else {
                CallFailure::Other(e)
            }
        })?;
        match &tool.target {
            Target::Sessions => {
                let sessions: Vec<Value> = self
                    .sessions
                    .iter()
                    .map(|s| {
                        json!({
                            "pid": s.descriptor.pid,
                            "url": s.descriptor.url,
                            "application": s.describe["moonglow"]
                        })
                    })
                    .collect();
                Ok(json!({"sessions": sessions, "diagnostics": self.diagnostics}))
            }
            Target::Jobs => jobs.manage(input).map_err(Into::into),
            Target::Agent(sessions) => {
                let selected = if tool.input["properties"].get("session").is_some() {
                    let pid = input["session"].as_str().ok_or("session required")?;
                    sessions.iter().find(|s| s.pid.to_string() == pid).ok_or("Unknown session")?
                } else {
                    sessions.first().ok_or("Missing session")?
                };
                let mut body = input.clone();
                if let Some(obj) = body.as_object_mut() {
                    obj.remove("session");
                }
                let reply = selected.response(
                    &format!("/v1/call/{name}"),
                    Some(&body),
                    selected.revision.as_deref(),
                )?;
                if matches!(reply.status, 404 | 412) {
                    return Err(CallFailure::Refresh(format!(
                        "Agent HTTP {}: refresh operation schema",
                        reply.status
                    )));
                }
                let value = reply.json()?;
                if value["ok"] != true {
                    return Err(value["error"].as_str().unwrap_or("Agent call failed").into());
                }
                Ok(value["result"].clone())
            }
            Target::Cli(command) => {
                if command["longRunning"] == true && input["background"] != true {
                    return Err("Long-running commands require background:true".into());
                }
                let argv = arguments(command, input, config)?;
                let run = Running::start(
                    &config.mg,
                    &argv,
                    input["timeout_ms"].as_u64().unwrap_or(120000),
                )?;
                if input["background"] == true {
                    jobs.start(run).map_err(Into::into)
                } else {
                    run.wait().map_err(Into::into)
                }
            }
        }
    }
}

enum CallFailure {
    Refresh(String),
    Other(String),
}
impl From<String> for CallFailure {
    fn from(value: String) -> Self {
        Self::Other(value)
    }
}
impl From<&str> for CallFailure {
    fn from(value: &str) -> Self {
        Self::Other(value.into())
    }
}
