use crate::{config::Config, https::LIMIT};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

type OutputThread = JoinHandle<Result<String, String>>;
pub struct Running {
    pid: u32,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<Value, String>>>,
    result: Option<Result<Value, String>>,
}
fn capture(mut pipe: impl Read + Send + 'static) -> OutputThread {
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buffer = [0; 8192];
        let mut truncated = false;
        loop {
            let n = pipe.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            let take = n.min(LIMIT - kept.len());
            kept.extend_from_slice(&buffer[..take]);
            truncated |= take < n;
        }
        let mut text = String::from_utf8_lossy(&kept).into_owned();
        if truncated {
            text.push_str("\n[output truncated]");
        }
        Ok(text)
    })
}
impl Running {
    pub fn start(program: &Path, args: &[String], timeout: u64) -> Result<Self, String> {
        let mut cmd = Command::new(program);
        cmd.env_remove("MOONGLOW_AI_TOKEN")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd.spawn().map_err(|e| format!("Cannot start CLI: {e}"))?;
        let out = child.stdout.take().ok_or("Missing CLI stdout")?;
        let err = child.stderr.take().ok_or("Missing CLI stderr")?;
        let pid = child.id();
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let worker = thread::spawn(move || {
            let out = capture(out);
            let err = capture(err);
            let started = Instant::now();
            let mut expired = false;
            let mut stopped = false;
            let status = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => {}
                    Err(e) => {
                        Self::kill(&mut child);
                        return Err(e.to_string());
                    }
                }
                stopped = signal.load(Ordering::Acquire);
                expired = started.elapsed() >= Duration::from_millis(timeout);
                if stopped || expired {
                    Self::kill(&mut child);
                    break child.wait().map_err(|e| e.to_string())?;
                }
                thread::sleep(Duration::from_millis(10));
            };
            // A trusted CLI must close its inherited pipes when it exits. Bound
            // output collection too, so a stray descendant cannot hang MCP.
            let drain = Instant::now();
            while !out.is_finished() || !err.is_finished() {
                if drain.elapsed() > Duration::from_secs(1) {
                    return Err("CLI exited but a descendant kept its output pipes open".into());
                }
                thread::sleep(Duration::from_millis(5));
            }
            let stdout = out.join().map_err(|_| "Output reader failed")??;
            let stderr = err.join().map_err(|_| "Output reader failed")??;
            let output = serde_json::from_str::<Value>(&stdout).unwrap_or_else(|_| json!(stdout));
            Ok(json!({
                "running": false,
                "ok": status.success() && !expired && !stopped,
                "exitCode": status.code(),
                "timedOut": expired,
                "stopped": stopped,
                "stdout": output,
                "stderr": stderr
            }))
        });
        Ok(Self { pid, cancel, worker: Some(worker), result: None })
    }
    fn kill(child: &mut Child) {
        // Only a child handle owned by this adapter authorizes termination.
        if matches!(child.try_wait(), Ok(None)) {
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                let _ = Command::new("taskkill")
                    .args(["/PID", &child.id().to_string(), "/T", "/F"])
                    .creation_flags(0x08000000)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            #[cfg(unix)]
            {
                let _ = Command::new("kill")
                    .args(["-KILL", "--", &format!("-{}", child.id())])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    pub fn poll(&mut self, stop: bool) -> Result<Value, String> {
        if let Some(value) = &self.result {
            return value.clone();
        }
        if stop {
            self.cancel.store(true, Ordering::Release);
        }
        if !stop && self.worker.as_ref().is_some_and(|w| !w.is_finished()) {
            return Ok(json!({"running":true,"pid":self.pid}));
        }
        let worker = self.worker.take().ok_or("Missing process worker")?;
        let value = worker.join().map_err(|_| "Process worker failed".to_owned()).and_then(|v| v);
        self.result = Some(value.clone());
        value
    }
    pub fn wait(mut self) -> Result<Value, String> {
        loop {
            let value = self.poll(false)?;
            if value["running"] == false {
                return Ok(value);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Default)]
pub struct Jobs {
    next: u64,
    running: BTreeMap<String, Running>,
}
impl Jobs {
    pub fn start(&mut self, run: Running) -> Result<Value, String> {
        if self.running.len() >= 32 {
            return Err("Job history full; forget completed jobs before starting more".into());
        }
        self.next += 1;
        let id = self.next.to_string();
        let pid = run.pid;
        self.running.insert(id.clone(), run);
        Ok(json!({"job":id,"running":true,"pid":pid}))
    }
    pub fn manage(&mut self, input: &Value) -> Result<Value, String> {
        let action = input["action"].as_str().unwrap_or("list");
        if action == "list" {
            let mut list = Vec::new();
            for (id, run) in &mut self.running {
                list.push(json!({"job":id,"status":run.poll(false)?}));
            }
            return Ok(json!({"jobs":list}));
        }
        let id = input["job"].as_str().ok_or("A job id is required")?;
        let run = self.running.get_mut(id).ok_or("Unknown owned job")?;
        let result = run.poll(action == "stop")?;
        if action == "forget" {
            if result["running"] == true {
                return Err("Stop the job before forgetting it".into());
            }
            self.running.remove(id);
        }
        Ok(result)
    }
}

fn safe_path(value: &str, roots: &[PathBuf]) -> Result<(), String> {
    let p = Path::new(value);
    let absolute = if p.is_absolute() {
        p.to_owned()
    } else {
        std::env::current_dir().map_err(|e| e.to_string())?.join(p)
    };
    // Resolve existing ancestors first, including symlinks. Never normalize away a symlink/.. pair.
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                if !resolved.pop() {
                    return Err("Path escapes its root".into());
                }
            }
            Component::CurDir => {}
            other => resolved.push(other.as_os_str()),
        }
        match resolved.symlink_metadata() {
            Ok(_) => resolved = resolved.canonicalize().map_err(|e| e.to_string())?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    if !roots.iter().any(|root| resolved.starts_with(root)) {
        return Err("Path is outside configured roots".into());
    }
    Ok(())
}
fn scalar(v: &Value) -> Result<String, String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        _ => Err("CLI argument must be a scalar".into()),
    }
}
pub fn arguments(command: &Value, input: &Value, config: &Config) -> Result<Vec<String>, String> {
    if command["writes"] == true && !config.write {
        return Err("CLI writes require --allow-write or MOONGLOW_MCP_WRITE=1".into());
    }
    let mut args = vec!["--json".to_owned()];
    for part in command["path"].as_array().ok_or("CLI path missing")? {
        let text = part.as_str().ok_or("Invalid CLI path")?;
        if text.starts_with('-') || text.is_empty() {
            return Err("Invalid CLI path".into());
        }
        args.push(text.into());
    }
    let mut mappings =
        command["argv"].as_array().ok_or("CLI argv missing")?.iter().collect::<Vec<_>>();
    mappings.sort_by_key(|m| m["position"].as_u64().unwrap_or(0));
    for m in mappings {
        let name = m["input"].as_str().ok_or("Invalid CLI mapping")?;
        let Some(value) = input.get(name) else {
            continue;
        };
        let flag = m["flag"].as_str();
        if flag.is_some_and(|f| !f.starts_with('-') || f.contains(['=', '\0'])) {
            return Err("Invalid CLI flag".into());
        }
        match m["action"].as_str() {
            Some("set_true" | "set_false") => {
                if value.as_bool().ok_or("Expected boolean flag")? == (m["action"] == "set_true") {
                    args.push(flag.ok_or("Boolean flag missing")?.into());
                }
                continue;
            }
            Some("count") => {
                let n = value
                    .as_u64()
                    .filter(|n| *n <= 100)
                    .ok_or("Flag count must be between 0 and 100")?;
                for _ in 0..n {
                    args.push(flag.ok_or("Count flag missing")?.into());
                }
                continue;
            }
            Some("value") => {}
            _ => return Err("Unsupported CLI argument action".into()),
        }
        if m["last"] == true {
            args.push("--".into());
        }
        let values = if let Some(array) = value.as_array() {
            array.iter().collect::<Vec<_>>()
        } else {
            vec![value]
        };
        for value in values {
            let s = scalar(value)?;
            if m["path"] == true {
                safe_path(&s, &config.roots)?;
            }
            if let Some(flag) = flag {
                args.push(format!("{flag}={s}"));
            } else {
                if s.starts_with('-') && m["last"] != true {
                    return Err("Positional arguments cannot start with a dash".into());
                }
                args.push(s);
            }
        }
    }
    Ok(args)
}
