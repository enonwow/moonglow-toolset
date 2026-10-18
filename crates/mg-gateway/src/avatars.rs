//! Avatar images for presence chips. The gateway fetches the advertised public HTTPS URL
//! (rules: `mg_live::avatar`), re-encodes it as a small PNG and serves it to authenticated
//! clients, so a DM's Moonglow never connects to a third-party host.
use crate::App;
use axum::{
    extract::{Path as UrlPath, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use std::{
    collections::BTreeMap,
    fmt::Debug,
    io::{Cursor, Read},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Largest origin body accepted.
pub const MAX_FETCH_BYTES: u64 = 512 * 1024;
/// Served images fit in this many pixels on each side.
pub const MAX_EDGE: u32 = 128;
const MAX_SOURCE_EDGE: u32 = 1024;
const MAX_ALLOC: usize = 8 * 1024 * 1024;
const CACHE_ENTRIES: usize = 256;
const CACHE_BYTES: usize = 8 * 1024 * 1024;
const CACHE_TTL: Duration = Duration::from_secs(300);

/// Where avatar bytes come from. The default is the public internet; hosts and tests may
/// substitute their own.
pub trait AvatarSource: Debug + Send + Sync + 'static {
    /// Blocking. At most `MAX_FETCH_BYTES` of body; any failure is an error.
    fn fetch(&self, url: &url::Url) -> Result<Vec<u8>, String>;
}
/// HTTPS to public addresses only: the resolver drops every non-public address at connect
/// time (so DNS cannot point into the gateway's network), no redirects, short timeouts, no
/// credentials or cookies.
#[derive(Debug, Default)]
pub struct HttpsSource;
impl AvatarSource for HttpsSource {
    fn fetch(&self, url: &url::Url) -> Result<Vec<u8>, String> {
        let agent = ureq::AgentBuilder::new()
            .redirects(0)
            .resolver(|addr: &str| {
                use std::net::ToSocketAddrs;
                let found: Vec<_> = addr
                    .to_socket_addrs()?
                    .filter(|a| mg_live::avatar::public_address(a.ip()))
                    .collect();
                if found.is_empty() {
                    return Err(std::io::Error::other("avatar host is not public"));
                }
                Ok(found)
            })
            .timeout(Duration::from_secs(10))
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(Duration::from_secs(5))
            .build();
        let response = agent.get(url.as_str()).call().map_err(|_| "origin request failed")?;
        if response.status() != 200 {
            return Err("origin status".into());
        }
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_FETCH_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "origin read failed")?;
        if bytes.len() as u64 > MAX_FETCH_BYTES {
            return Err("origin body too large".into());
        }
        Ok(bytes)
    }
}

/// PNG or JPEG in, PNG of at most `MAX_EDGE` pixels out. Bounded in bytes, pixels and memory.
pub fn transcode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    const BAD: &str = "unusable image";
    if bytes.len() as u64 > MAX_FETCH_BYTES {
        return Err(BAD.into());
    }
    let rgba = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let mut decoder =
            png::Decoder::new_with_limits(Cursor::new(bytes), png::Limits { bytes: MAX_ALLOC });
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().map_err(|_| BAD)?;
        let info = reader.info();
        if info.width == 0
            || info.height == 0
            || info.width > MAX_SOURCE_EDGE
            || info.height > MAX_SOURCE_EDGE
        {
            return Err(BAD.into());
        }
        let size = reader.output_buffer_size().filter(|s| *s <= MAX_ALLOC / 2).ok_or(BAD)?;
        let mut data = vec![0; size];
        let info = reader.next_frame(&mut data).map_err(|_| BAD)?;
        let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
        for p in data[..info.buffer_size()].chunks_exact(info.color_type.samples()) {
            rgba.extend_from_slice(&match info.color_type {
                png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
                png::ColorType::Rgb => [p[0], p[1], p[2], 255],
                png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
                png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
                png::ColorType::Indexed => return Err(BAD.into()),
            });
        }
        image::RgbaImage::from_raw(info.width, info.height, rgba).ok_or(BAD)?
    } else {
        let mut reader =
            image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Jpeg);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_SOURCE_EDGE);
        limits.max_image_height = Some(MAX_SOURCE_EDGE);
        limits.max_alloc = Some(MAX_ALLOC as u64);
        reader.limits(limits);
        reader.decode().map_err(|_| BAD)?.into_rgba8()
    };
    let small = if rgba.width() > MAX_EDGE || rgba.height() > MAX_EDGE {
        image::DynamicImage::ImageRgba8(rgba).thumbnail(MAX_EDGE, MAX_EDGE).into_rgba8()
    } else {
        rgba
    };
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, small.width(), small.height());
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut w| w.write_image_data(small.as_raw()))
            .map_err(|_| BAD)?;
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy)]
pub struct AvatarLimits {
    pub global: usize,
    pub per_principal: usize,
}
impl Default for AvatarLimits {
    fn default() -> Self {
        Self { global: 8, per_principal: 2 }
    }
}
#[derive(Debug, Default)]
struct Usage {
    active: usize,
    principals: BTreeMap<String, usize>,
}
#[derive(Debug)]
struct Permit {
    usage: Arc<Mutex<Usage>>,
    principal: String,
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut usage = self.usage.lock().unwrap_or_else(|e| e.into_inner());
        usage.active -= 1;
        if let Some(count) = usage.principals.get_mut(&self.principal) {
            *count -= 1;
            if *count == 0 {
                usage.principals.remove(&self.principal);
            }
        }
    }
}
#[derive(Debug)]
struct Entry {
    png: Arc<[u8]>,
    at: Instant,
    used: u64,
}
#[derive(Debug, Default)]
struct Cache {
    entries: BTreeMap<(String, String), Entry>,
    bytes: usize,
    clock: u64,
}
impl Cache {
    fn remove(&mut self, key: &(String, String)) {
        if let Some(old) = self.entries.remove(key) {
            self.bytes -= old.png.len();
        }
    }
}

/// Fetch, transcode and cache avatars; part of `Config`.
#[derive(Debug)]
pub struct Avatars {
    source: Arc<dyn AvatarSource>,
    limits: AvatarLimits,
    cache: Mutex<Cache>,
    usage: Arc<Mutex<Usage>>,
}
impl Default for Avatars {
    fn default() -> Self {
        Self::new(Arc::new(HttpsSource), AvatarLimits::default())
    }
}
impl Avatars {
    pub fn new(source: Arc<dyn AvatarSource>, limits: AvatarLimits) -> Self {
        Self { source, limits, cache: Mutex::default(), usage: Arc::default() }
    }
    fn acquire(&self, principal: &str) -> Option<Permit> {
        let mut usage = self.usage.lock().unwrap_or_else(|e| e.into_inner());
        if usage.active >= self.limits.global
            || usage.principals.get(principal).copied().unwrap_or(0) >= self.limits.per_principal
        {
            return None;
        }
        usage.active += 1;
        *usage.principals.entry(principal.into()).or_default() += 1;
        Some(Permit { usage: self.usage.clone(), principal: principal.into() })
    }
    fn cached(&self, id: &str, url: &str) -> Option<Arc<[u8]>> {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.clock += 1;
        let clock = cache.clock;
        let key = (id.to_owned(), url.to_owned());
        if cache.entries.get(&key)?.at.elapsed() >= CACHE_TTL {
            cache.remove(&key);
            return None;
        }
        let entry = cache.entries.get_mut(&key)?;
        entry.used = clock;
        Some(entry.png.clone())
    }
    fn store(&self, id: &str, url: &str, png: Arc<[u8]>) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.clock += 1;
        let used = cache.clock;
        // One entry per principal: a changed URL replaces the old image.
        let stale: Vec<_> = cache.entries.keys().filter(|(i, _)| i == id).cloned().collect();
        for key in stale {
            cache.remove(&key);
        }
        while cache.entries.len() >= CACHE_ENTRIES || cache.bytes + png.len() > CACHE_BYTES {
            let Some(oldest) =
                cache.entries.iter().min_by_key(|(_, e)| e.used).map(|(k, _)| k.clone())
            else {
                break;
            };
            cache.remove(&oldest);
        }
        cache.bytes += png.len();
        cache.entries.insert((id.into(), url.into()), Entry { png, at: Instant::now(), used });
    }
    fn load(&self, id: &str, url: &url::Url) -> Result<Arc<[u8]>, String> {
        let png: Arc<[u8]> = transcode(&self.source.fetch(url)?)?.into();
        self.store(id, url.as_str(), png.clone());
        Ok(png)
    }
}

/// `GET /avatars/<principal id>`: any authenticated DM or AI bearer, never a player.
pub(crate) async fn handle(
    State(app): State<App>,
    UrlPath(id): UrlPath<String>,
    headers: HeaderMap,
) -> Response {
    let (_, principal) = match crate::authenticate(&app, &headers).await {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let lookup = id.clone();
    let url = match app.core.with_gateway(move |g| g.ai_avatar(&lookup)).await {
        Ok(url) => url.and_then(|u| mg_live::avatar::avatar_url(&u)),
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let Some(url) = url else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let avatars = app.config.avatars.clone();
    let png = match avatars.cached(&id, url.as_str()) {
        Some(png) => png,
        None => {
            let Some(permit) = avatars.acquire(&principal.id) else {
                return StatusCode::TOO_MANY_REQUESTS.into_response();
            };
            // The worker owns its permit even if the HTTP task is cancelled.
            let loaded = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                avatars.load(&id, &url)
            })
            .await;
            match loaded {
                Ok(Ok(png)) => png,
                _ => return StatusCode::BAD_GATEWAY.into_response(),
            }
        }
    };
    (
        [
            ("cache-control", "private, max-age=300"),
            ("content-type", "image/png"),
            ("x-content-type-options", "nosniff"),
        ],
        png.to_vec(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Fixed(Vec<u8>);
    impl AvatarSource for Fixed {
        fn fetch(&self, _: &url::Url) -> Result<Vec<u8>, String> {
            Ok(self.0.clone())
        }
    }
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&vec![127; width as usize * height as usize * 4])
                .unwrap();
        }
        bytes
    }
    fn size(bytes: &[u8]) -> (u32, u32) {
        let reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
        (reader.info().width, reader.info().height)
    }

    #[test]
    fn transcode_bounds_bytes_pixels_and_output() {
        assert_eq!(size(&transcode(&png(300, 150)).unwrap()), (128, 64));
        assert_eq!(size(&transcode(&png(16, 4)).unwrap()), (16, 4));
        assert!(transcode(&png(1025, 1)).is_err());
        assert!(transcode(b"garbage").is_err());
        assert!(transcode(&png(16, 16)[..20]).is_err());
        assert!(transcode(&vec![0; MAX_FETCH_BYTES as usize + 1]).is_err());
    }

    #[test]
    fn real_fetcher_never_connects_to_loopback_even_by_name() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = url::Url::parse(&format!("https://localhost:{port}/a.png")).unwrap();
        assert!(HttpsSource.fetch(&url).is_err());
        assert!(listener.accept().is_err(), "the connection must be refused before connect");
    }

    #[test]
    fn cache_is_bounded_keyed_by_url_and_expires() {
        let a = Avatars::new(Arc::new(Fixed(png(8, 8))), AvatarLimits::default());
        let url = url::Url::parse("https://cdn.example.org/a.png").unwrap();
        a.load("ai:one", &url).unwrap();
        assert!(a.cached("ai:one", url.as_str()).is_some());
        assert!(a.cached("ai:one", "https://cdn.example.org/b.png").is_none());
        let other = url::Url::parse("https://cdn.example.org/b.png").unwrap();
        a.load("ai:one", &other).unwrap();
        assert_eq!(a.cache.lock().unwrap().entries.len(), 1, "a new URL replaces the entry");
        for i in 0..CACHE_ENTRIES + 10 {
            a.load(&format!("ai:{i}"), &url).unwrap();
        }
        {
            let cache = a.cache.lock().unwrap();
            assert!(cache.entries.len() <= CACHE_ENTRIES && cache.bytes <= CACHE_BYTES);
        }
        a.cache.lock().unwrap().entries.values_mut().for_each(|e| {
            e.at = Instant::now() - CACHE_TTL - Duration::from_secs(1);
        });
        assert!(a.cached("ai:5", url.as_str()).is_none());
    }

    #[test]
    fn permits_bound_global_and_per_principal_and_release() {
        let a = Avatars::new(Arc::new(Fixed(vec![])), AvatarLimits { global: 3, per_principal: 2 });
        let p1 = a.acquire("dm:a").unwrap();
        let _p2 = a.acquire("dm:a").unwrap();
        assert!(a.acquire("dm:a").is_none());
        let _p3 = a.acquire("dm:b").unwrap();
        assert!(a.acquire("dm:c").is_none());
        drop(p1);
        assert!(a.acquire("dm:c").is_some());
    }
}
