//! Read-only manifest planning. Transfer and activation remain in mg-live-link.
use mg_core::sha1::{hex, sha1};
use mg_live::resource_files::ResourceGeneration;
use mg_live_link::{Gateway, SessionToken, resources::Cancel};
use mg_module::nwsync_cache::Limits;
use std::{collections::BTreeMap, io::Read, path::Path, time::Duration};

#[derive(Debug, Clone, Default)]
pub(crate) struct Plan {
    pub missing_bytes: u64,
    pub transfers: u64,
    pub disk_growth: u64,
}

pub(crate) fn plan(
    origin: &str,
    gateway: &Gateway,
    token: &SessionToken,
    root: &Path,
    generation: &ResourceGeneration,
    limits: Limits,
    cancel: &Cancel,
) -> Result<Plan, String> {
    generation.validate()?;
    let mut origin = url::Url::parse(origin).map_err(|_| "Invalid gateway URL")?;
    if origin.scheme() == "wss" {
        origin.set_scheme("https").map_err(|_| "Invalid scheme")?;
    }
    let module = origin.join(&generation.module_url).map_err(|_| "Invalid module URL")?;
    if module.origin() != origin.origin() || module.path() != "/files/v1/" {
        return Err("Invalid module resource endpoint".into());
    }
    let haks = url::Url::parse(&generation.haks_url).map_err(|_| "Invalid hak URL")?;
    for url in [&module, &haks] {
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.path().ends_with('/')
        {
            return Err("Invalid resource repository URL".into());
        }
    }
    let pinned = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(5))
        .tls_config(gateway.tls_config().map_err(|e| e.to_string())?)
        .build();
    let public = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(5))
        .build();
    let mut plan = Plan::default();
    let mut hashes = BTreeMap::new();
    let mut decoded = 0u64;
    let mut module_bytes = 0u64;
    for (base, hash, authenticated) in
        [(&haks, &generation.haks_manifest, false), (&module, &generation.module_manifest, true)]
    {
        check(cancel)?;
        let cached = root.join("cache/manifests").join(hash);
        let raw = if cached.exists() {
            regular_read(&cached, limits.manifest_bytes, cancel)?
        } else {
            let agent =
                if authenticated || base.origin() == origin.origin() { &pinned } else { &public };
            let url =
                base.join(&format!("manifests/{hash}")).map_err(|_| "Invalid manifest URL")?;
            let mut request = agent.get(url.as_str());
            if authenticated {
                request = request.set("Authorization", &format!("Bearer {}", token.expose()));
            }
            let response = request.call().map_err(|e| match e {
                ureq::Error::Status(code, _) => format!("resource HTTP {code}"),
                ureq::Error::Transport(_) => "resource transport unavailable".into(),
            })?;
            if response.status() != 200 {
                return Err("Resource manifest request failed".into());
            }
            let raw = bounded(response.into_reader(), limits.manifest_bytes, cancel)?;
            plan.transfers += 1;
            plan.disk_growth += raw.len() as u64;
            raw
        };
        if hex(&sha1(&raw)) != *hash {
            return Err("Resource manifest SHA-1 mismatch".into());
        }
        let entries = mg_module::nwsync::read_manifest(&raw)?;
        if entries.len() > limits.entries {
            return Err("Resource manifest entry limit".into());
        }
        for (_, hash, size) in entries {
            decoded = decoded.checked_add(u64::from(size)).ok_or("Resource size overflow")?;
            if size as usize > limits.resource_bytes || decoded > limits.total_bytes {
                return Err("Resource generation size limit".into());
            }
            if authenticated {
                module_bytes += u64::from(size);
            }
            if hashes.insert(hash, size).is_some_and(|old| old != size) {
                return Err("Inconsistent resource size".into());
            }
        }
    }
    for (hash, size) in hashes {
        check(cancel)?;
        let path = root.join("cache/blobs").join(hex(&hash));
        if path.exists() {
            let raw = regular_read(&path, size as usize, cancel)?;
            if raw.len() != size as usize || sha1(&raw) != hash {
                return Err("Cached resource SHA-1/size mismatch".into());
            }
        } else {
            plan.missing_bytes += u64::from(size);
            plan.transfers += 1;
        }
    }
    plan.disk_growth += plan.missing_bytes;
    if !root.join("generations").join(generation.id()).exists() {
        plan.disk_growth += module_bytes;
    }
    plan.disk_growth +=
        serde_json::to_vec(generation).map_err(|_| "Invalid descriptor")?.len() as u64;
    Ok(plan)
}
fn check(cancel: &Cancel) -> Result<(), String> {
    if cancel.is_cancelled() { Err("download cancelled".into()) } else { Ok(()) }
}
fn regular_read(path: &Path, max: usize, cancel: &Cancel) -> Result<Vec<u8>, String> {
    if !std::fs::symlink_metadata(path).map_err(|_| "Cannot inspect cached resource")?.is_file() {
        return Err("Cached resource is not a regular file".into());
    }
    bounded(std::fs::File::open(path).map_err(|_| "Cannot read cached resource")?, max, cancel)
}
fn bounded(mut reader: impl Read, max: usize, cancel: &Cancel) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut buf = [0; 16384];
    loop {
        check(cancel)?;
        let room = (max + 1).saturating_sub(bytes.len()).min(buf.len());
        if room == 0 {
            return Err("Resource manifest exceeds limit".into());
        }
        let n = reader.read(&mut buf[..room]).map_err(|_| "Cannot read resource manifest")?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        if bytes.len() > max {
            return Err("Resource manifest exceeds limit".into());
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_manifest_reader_rejects_overflow_and_cancellation() {
        assert!(bounded(&b"12345"[..], 4, &Cancel::default()).is_err());
        let cancel = Cancel::default();
        cancel.cancel();
        assert!(bounded(&b""[..], 4, &cancel).is_err());
        assert_eq!(bounded(&b"1234"[..], 4, &Cancel::default()).unwrap(), b"1234");
    }
}
