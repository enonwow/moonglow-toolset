//! Small, bounded avatar cache shared by the AI editor and presence chips.
//!
//! Images are loaded only from the connected gateway (`GET /avatars/<id>`, with the session
//! bearer and the link's own pin or roots). The third-party URL a gateway advertises is
//! display text: this module never resolves or connects to it.
use mg_live_link::AvatarClient;
use std::{
    collections::BTreeMap,
    io::Cursor,
    sync::{Arc, Mutex},
};

const MAX_BYTES: usize = 256 * 1024;
/// The gateway serves at most this many pixels per side.
const MAX_EDGE: u32 = 128;
const TEXTURE_EDGE: u32 = 64;
const MAX_ENTRIES: usize = 64;
const MAX_JOBS: usize = 4;

pub(crate) fn valid_url(text: &str) -> bool {
    mg_live::avatar::avatar_url(text).is_some()
}

/// An avatar as advertised by the gateway: the route that serves the image, and the source URL
/// (text for display; it also tells a changed image from an unchanged one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ref {
    path: String,
    url: String,
}
impl Ref {
    pub(crate) fn new(path: Option<&str>, url: Option<&str>) -> Option<Self> {
        let (path, url) = (path?, url?);
        (valid_url(url) && path.starts_with("/avatars/") && path.len() <= 256)
            .then(|| Self { path: path.into(), url: url.into() })
    }
}

#[derive(Clone)]
pub(crate) enum Avatar {
    Loading,
    Ready(egui::TextureHandle),
    Unavailable,
}
type Loader = Arc<dyn Fn(&str) -> Result<Vec<u8>, ()> + Send + Sync>;
#[derive(Default)]
struct Cache {
    connection: Option<AvatarClient>,
    loader: Option<Loader>,
    entries: BTreeMap<Ref, (u64, Avatar)>,
    clock: u64,
    jobs: usize,
}
impl PartialOrd for Ref {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Ref {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (&self.path, &self.url).cmp(&(&other.path, &other.url))
    }
}
fn cache(ctx: &egui::Context) -> Arc<Mutex<Cache>> {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Arc<Mutex<Cache>>>(egui::Id::new("live-avatar-cache")).clone()
    })
}
/// Call every frame with the active link's client (`None` without one). A different
/// connection drops every cached image.
pub(crate) fn set_client(ctx: &egui::Context, client: Option<AvatarClient>) {
    let shared = cache(ctx);
    let mut cache = shared.lock().unwrap_or_else(|e| e.into_inner());
    let same = match (&cache.connection, &client) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_connection(b),
        _ => false,
    };
    if same {
        return;
    }
    cache.entries.clear();
    cache.loader =
        client.clone().map(|c| -> Loader { Arc::new(move |path| c.fetch(path).map_err(|_| ())) });
    cache.connection = client;
}
pub(crate) fn retry(ctx: &egui::Context, avatar: &Ref) {
    let cache = cache(ctx);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    if cache.entries.get(avatar).is_some_and(|(_, a)| !matches!(a, Avatar::Loading)) {
        cache.entries.remove(avatar);
    }
}
pub(crate) fn get(ctx: &egui::Context, avatar: &Ref) -> Avatar {
    let shared = cache(ctx);
    let loader = {
        let mut cache = shared.lock().unwrap_or_else(|e| e.into_inner());
        cache.clock = cache.clock.wrapping_add(1);
        let now = cache.clock;
        if let Some((used, found)) = cache.entries.get_mut(avatar) {
            *used = now;
            return found.clone();
        }
        let Some(loader) = cache.loader.clone() else {
            return Avatar::Unavailable;
        };
        if cache.jobs >= MAX_JOBS {
            return Avatar::Loading;
        }
        if cache.entries.len() >= MAX_ENTRIES {
            let oldest = cache
                .entries
                .iter()
                .filter(|(_, (_, a))| !matches!(a, Avatar::Loading))
                .min_by_key(|(_, (used, _))| *used)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                cache.entries.remove(&oldest);
            } else {
                return Avatar::Loading;
            }
        }
        cache.entries.insert(avatar.clone(), (now, Avatar::Loading));
        cache.jobs += 1;
        loader
    };
    let key = avatar.clone();
    let worker_cache = shared.clone();
    let worker_ctx = ctx.clone();
    let result = std::thread::Builder::new().name("live-avatar".into()).spawn(move || {
        let decoded = loader(&key.path).and_then(|bytes| decode(&bytes));
        let found = decoded.map_or(Avatar::Unavailable, |image| {
            Avatar::Ready(worker_ctx.load_texture(
                "Live avatar",
                image,
                egui::TextureOptions::LINEAR,
            ))
        });
        let mut cache = worker_cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.jobs -= 1;
        if let Some((_, value)) = cache.entries.get_mut(&key) {
            *value = found;
        }
        drop(cache);
        worker_ctx.request_repaint();
    });
    if result.is_err() {
        let mut cache = shared.lock().unwrap_or_else(|e| e.into_inner());
        cache.jobs -= 1;
        if let Some((_, value)) = cache.entries.get_mut(avatar) {
            *value = Avatar::Unavailable;
        }
        return Avatar::Unavailable;
    }
    Avatar::Loading
}
/// The gateway sends PNGs of at most 128 by 128 pixels; anything else is refused.
fn decode(bytes: &[u8]) -> Result<egui::ColorImage, ()> {
    if bytes.len() > MAX_BYTES {
        return Err(());
    }
    let mut decoder =
        png::Decoder::new_with_limits(Cursor::new(bytes), png::Limits { bytes: 4 * 1024 * 1024 });
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| ())?;
    let info = reader.info();
    if info.width == 0 || info.height == 0 || info.width > MAX_EDGE || info.height > MAX_EDGE {
        return Err(());
    }
    let size = reader
        .output_buffer_size()
        .filter(|size| *size <= MAX_EDGE as usize * MAX_EDGE as usize * 4)
        .ok_or(())?;
    let mut data = vec![0; size];
    let info = reader.next_frame(&mut data).map_err(|_| ())?;
    let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
    for p in data[..info.buffer_size()].chunks_exact(info.color_type.samples()) {
        rgba.extend_from_slice(&match info.color_type {
            png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
            png::ColorType::Rgb => [p[0], p[1], p[2], 255],
            png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
            png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
            png::ColorType::Indexed => return Err(()),
        });
    }
    let rgba = image::RgbaImage::from_raw(info.width, info.height, rgba).ok_or(())?;
    let small = if rgba.width() > TEXTURE_EDGE || rgba.height() > TEXTURE_EDGE {
        image::DynamicImage::ImageRgba8(rgba).thumbnail(TEXTURE_EDGE, TEXTURE_EDGE).into_rgba8()
    } else {
        rgba
    };
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [small.width() as usize, small.height() as usize],
        small.as_raw(),
    ))
}
pub(crate) fn paint(ui: &egui::Ui, rect: egui::Rect, avatar: Option<&Ref>) -> bool {
    if let Some(avatar) = avatar
        && let Avatar::Ready(texture) = get(ui.ctx(), avatar)
    {
        ui.painter().image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            egui::Color32::WHITE,
        );
        true
    } else {
        ui.painter().circle_filled(
            rect.center(),
            rect.width() * 0.35,
            egui::Color32::from_rgb(112, 147, 163),
        );
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    fn png(width: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&vec![127; width as usize * 4])
                .unwrap();
        }
        bytes
    }
    fn avatar(path: &str) -> Ref {
        Ref::new(Some(path), Some("https://cdn.example.org/portrait.png")).unwrap()
    }
    #[test]
    fn urls_require_bounded_https_without_credentials() {
        assert!(valid_url("https://example.invalid/portrait.png"));
        for bad in [
            "http://example.invalid/a",
            "file:///a",
            "data:image/png,a",
            "https://user:pass@example.invalid/a",
            "https://example.invalid/a b",
        ] {
            assert!(!valid_url(bad));
        }
        assert!(!valid_url(&format!("https://example.invalid/{}", "a".repeat(2048))));
    }
    #[test]
    fn only_gateway_avatar_routes_are_references() {
        let url = Some("https://cdn.example.org/a.png");
        assert!(Ref::new(Some("/avatars/ai:scribe"), url).is_some());
        for path in [
            None,
            Some("https://cdn.example.org/a.png"),
            Some("//cdn.example.org/avatars/x"),
            Some("/files/v1/x"),
            Some(""),
        ] {
            assert!(Ref::new(path, url).is_none(), "{path:?}");
        }
        assert!(
            Ref::new(Some("/avatars/ai:scribe"), Some("http://cdn.example.org/a.png")).is_none()
        );
        assert!(Ref::new(Some("/avatars/ai:scribe"), None).is_none());
    }
    #[test]
    fn decoder_bounds_encoded_bytes_pixels_and_output_texture() {
        assert_eq!(decode(&png(128)).unwrap().size, [64, 1]);
        assert_eq!(decode(&png(16)).unwrap().size, [16, 1]);
        assert!(decode(&png(129)).is_err());
        assert!(decode(b"garbage").is_err());
        assert!(decode(&png(16)[..20]).is_err());
        assert!(decode(&vec![0; MAX_BYTES + 1]).is_err());
    }
    #[test]
    fn nothing_loads_without_a_gateway_connection() {
        let ctx = egui::Context::default();
        assert!(matches!(get(&ctx, &avatar("/avatars/ai:a")), Avatar::Unavailable));
        assert!(cache(&ctx).lock().unwrap().entries.is_empty());
    }
    #[test]
    fn the_loader_is_asked_for_the_gateway_path_only_and_results_are_cached() {
        let ctx = egui::Context::default();
        let asked = Arc::new(Mutex::new(Vec::<String>::new()));
        let log = asked.clone();
        cache(&ctx).lock().unwrap().loader = Some(Arc::new(move |path| {
            log.lock().unwrap().push(path.into());
            Ok(png(16))
        }));
        let wanted = avatar("/avatars/ai:a");
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Avatar::Ready(texture) = get(&ctx, &wanted) {
                assert_eq!(texture.size(), [16, 1]);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        get(&ctx, &wanted);
        assert_eq!(*asked.lock().unwrap(), ["/avatars/ai:a"]);
        assert_eq!(cache(&ctx).lock().unwrap().entries.len(), 1);
    }
}
