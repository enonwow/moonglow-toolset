//! A sequential, bounded long-poll loop. Only event JSON goes to stdout.
use crate::{config::Config, discovery::Catalogue, process::Jobs};
use serde_json::{Value, json};
use std::io::{self, Write};

pub const USAGE: &str = "Usage: moonglow-mcp [tools | call TOOL [JSON] | watch [OPTIONS]]
watch: --session ID --since N --from players --kinds chat,damage --area RESREF
       --to-dm --count N (stop after N events)
Default watch starts now; --since replays retained events. Ctrl+C stops.
Common: --gateway URL [--gateway-pin SHA256] [--session-id ID]; token: MOONGLOW_AI_TOKEN
Legacy: --descriptor FILE --sessions DIR --mg PROGRAM --roots DIR --allow-write
MCP: no command for stdio; --http ADDRESS --http-token TOKEN [--http-allow-remote]";

#[derive(Default)]
struct Options {
    session: Option<String>,
    since: Option<u64>,
    count: Option<u64>,
    filter: serde_json::Map<String, Value>,
}
impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.iter();
        while let Some(flag) = args.next() {
            if flag == "--to-dm" {
                options.filter.insert("to_dm".into(), json!(true));
                continue;
            }
            let value = args.next().ok_or_else(|| format!("Missing value for {flag}"))?;
            match flag.as_str() {
                "--session" => options.session = Some(value.clone()),
                "--since" => {
                    options.since = Some(value.parse().map_err(|_| "Invalid event cursor")?)
                }
                "--count" => {
                    let count = value.parse().map_err(|_| "Invalid event count")?;
                    if count == 0 {
                        return Err("--count must be positive".into());
                    }
                    options.count = Some(count);
                }
                "--from" if value == "players" => {
                    options.filter.insert("from".into(), json!(value));
                }
                "--area" if !value.is_empty() => {
                    options.filter.insert("area".into(), json!(value));
                }
                "--kinds" => {
                    let kinds: Vec<_> = value.split(',').map(str::trim).collect();
                    if kinds.iter().any(|k| k.is_empty()) {
                        return Err("Empty event kind".into());
                    }
                    options.filter.insert("kinds".into(), json!(kinds));
                }
                _ => return Err(format!("Invalid watch option {flag} {value}\n{USAGE}")),
            }
        }
        Ok(options)
    }
}

pub fn run(config: Config, args: &[String]) -> Result<(), String> {
    let options = Options::parse(args)?;
    let (mut catalogue, name, session) = Catalogue::for_watch(&config, options.session.as_deref())?;
    let mut input = Value::Object(options.filter);
    if let Some(session) = session {
        input["session"] = json!(session);
    }
    let mut jobs = Jobs::default();
    let mut cursor = options.since.unwrap_or(0);
    // A one-row snapshot establishes the tail without printing history.
    if options.since.is_none() {
        input["since"] = json!(0);
        input["wait_ms"] = json!(0);
        input["limit"] = json!(1);
        let result = catalogue.call(&name, &input, &config, &mut jobs)?;
        cursor = result["last"].as_u64().ok_or("Event stream omitted last cursor")?;
        remember_boot(&mut input, &result)?;
    }
    input["wait_ms"] = json!(25000);
    input["limit"] = json!(500);
    let mut printed = 0;
    let mut output = io::stdout().lock();
    loop {
        input["since"] = json!(cursor);
        let result = catalogue
            .call(&name, &input, &config, &mut jobs)
            .map_err(|e| format!("Watch stopped after cursor {cursor}: {e}"))?;
        remember_boot(&mut input, &result)?;
        let (events, last) = batch(&result, cursor)?;
        for event in events {
            if let Err(e) = writeln!(output, "{event}").and_then(|_| output.flush()) {
                return if e.kind() == io::ErrorKind::BrokenPipe {
                    Ok(())
                } else {
                    Err(e.to_string())
                };
            }
            printed += 1;
            if options.count.is_some_and(|count| printed >= count) {
                return Ok(());
            }
        }
        cursor = last;
    }
}

fn remember_boot(input: &mut Value, result: &Value) -> Result<(), String> {
    let boot = result["boot"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("No live game boot; wait for a game report before watching")?;
    if input.get("boot").is_some_and(|old| old != boot) {
        return Err("Game boot changed; reconcile state and start a new watch".into());
    }
    input["boot"] = json!(boot);
    Ok(())
}

fn batch(result: &Value, since: u64) -> Result<(&Vec<Value>, u64), String> {
    let last = result["last"].as_u64().ok_or("Event stream omitted last cursor")?;
    if result["reset"] == true || last < since {
        return Err("Event cursor reset; reconcile state and start a new watch".into());
    }
    if result["lost"] == true {
        return Err(format!(
            "Event history lost after cursor {since}; reconcile state before resuming"
        ));
    }
    let events = result["events"].as_array().ok_or("Event stream omitted events")?;
    let mut previous = since;
    for event in events {
        let n = event["n"].as_u64().ok_or("Event omitted its cursor")?;
        if n <= previous || n > last {
            return Err("Event stream returned unordered cursors".into());
        }
        previous = n;
    }
    Ok((events, last))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boot_identity_must_remain_stable() {
        let mut input = json!({});
        remember_boot(&mut input, &json!({"boot":"one"})).unwrap();
        remember_boot(&mut input, &json!({"boot":"one"})).unwrap();
        assert!(remember_boot(&mut input, &json!({"boot":"two"})).is_err());
    }
    #[test]
    fn cursor_gaps_are_valid_but_loss_reset_and_replays_are_not() {
        assert!(batch(&json!({"events":[{"n":4},{"n":8}],"last":10}), 2).is_ok());
        for bad in [
            json!({"events":[],"last":1}),
            json!({"events":[],"last":9,"reset":true}),
            json!({"events":[],"last":9,"lost":true}),
            json!({"events":[{"n":3},{"n":3}],"last":4}),
            json!({"events":[{"n":5}],"last":4}),
        ] {
            assert!(batch(&bad, 2).is_err());
        }
    }
}
