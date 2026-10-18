//! Small, bounded HTTPS avatar cache shared by the AI editor and presence chips.
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    sync::{Arc, Mutex},
    time::Duration,
};

const MAX_BYTES: u64 = 512 * 1024;
const MAX_ENTRIES: usize = 64;
const MAX_JOBS: usize = 4;

pub(crate) fn valid_url(text: &str) -> bool {
    text.len() <= 2048
        && !text.chars().any(char::is_whitespace)
        && url::Url::parse(text).is_ok_and(|u| {
            u.scheme() == "https"
                && u.host_str().is_some()
                && u.username().is_empty()
                && u.password().is_none()
        })
}

#[derive(Clone)]
pub(crate) enum Avatar {
    Loading,
    Ready(egui::TextureHandle),
    Unavailable,
}
#[derive(Default)]
struct Cache {
    entries: BTreeMap<String, (u64, Avatar)>,
    clock: u64,
    jobs: usize,
}
fn cache(ctx: &egui::Context) -> Arc<Mutex<Cache>> {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Arc<Mutex<Cache>>>(egui::Id::new("live-avatar-cache")).clone()
    })
}
pub(crate) fn retry(ctx: &egui::Context, url: &str) {
    let cache = cache(ctx);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    if cache.entries.get(url).is_some_and(|(_, a)| !matches!(a, Avatar::Loading)) {
        cache.entries.remove(url);
    }
}
pub(crate) fn get(ctx: &egui::Context, url: &str) -> Avatar {
    if !valid_url(url) {
        return Avatar::Unavailable;
    }
    let shared = cache(ctx);
    {
        let mut cache = shared.lock().unwrap_or_else(|e| e.into_inner());
        cache.clock = cache.clock.wrapping_add(1);
        let now = cache.clock;
        if let Some((used, avatar)) = cache.entries.get_mut(url) {
            *used = now;
            return avatar.clone();
        }
        if cache.jobs >= MAX_JOBS {
            return Avatar::Loading;
        }
        if cache.entries.len() >= MAX_ENTRIES {
            let oldest = cache
                .entries
                .iter()
                .filter(|(_, (_, a))| !matches!(a, Avatar::Loading))
                .min_by_key(|(_, (used, _))| *used)
                .map(|(url, _)| url.clone());
            if let Some(oldest) = oldest {
                cache.entries.remove(&oldest);
            } else {
                return Avatar::Loading;
            }
        }
        cache.entries.insert(url.into(), (now, Avatar::Loading));
        cache.jobs += 1;
    }
    let ctx = ctx.clone();
    let address = url.to_owned();
    let worker_cache = shared.clone();
    let worker_ctx = ctx.clone();
    let result = std::thread::Builder::new().name("live-avatar".into()).spawn(move || {
        // Included bytes use the same bounded decoder (also useful for authored fixtures).
        let decoded = match worker_ctx.try_load_bytes(&address) {
            Ok(egui::load::BytesPoll::Ready { bytes, .. }) => decode(&bytes),
            _ => fetch(&address).and_then(|bytes| decode(&bytes)),
        };
        let avatar = decoded.map_or(Avatar::Unavailable, |image| {
            Avatar::Ready(worker_ctx.load_texture(
                "Live avatar",
                image,
                egui::TextureOptions::LINEAR,
            ))
        });
        let mut cache = worker_cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.jobs -= 1;
        if let Some((_, value)) = cache.entries.get_mut(&address) {
            *value = avatar;
        }
        drop(cache);
        worker_ctx.request_repaint();
    });
    if result.is_err() {
        let mut cache = shared.lock().unwrap_or_else(|e| e.into_inner());
        cache.jobs -= 1;
        if let Some((_, value)) = cache.entries.get_mut(url) {
            *value = Avatar::Unavailable;
        }
        return Avatar::Unavailable;
    }
    Avatar::Loading
}
fn fetch(url: &str) -> Result<Vec<u8>, ()> {
    // No gateway bearer, cookies, referrer or redirect to another URL.
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(10))
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(5))
        .build();
    let response = agent.get(url).call().map_err(|_| ())?;
    if response.status() != 200 {
        return Err(());
    }
    bounded(response.into_reader())
}
fn bounded(reader: impl Read) -> Result<Vec<u8>, ()> {
    let mut bytes = Vec::new();
    reader.take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|_| ())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(());
    }
    Ok(bytes)
}
fn decode(bytes: &[u8]) -> Result<egui::ColorImage, ()> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(());
    }
    let rgba = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let mut decoder = png::Decoder::new_with_limits(
            Cursor::new(bytes),
            png::Limits { bytes: 8 * 1024 * 1024 },
        );
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().map_err(|_| ())?;
        let info = reader.info();
        if info.width == 0 || info.height == 0 || info.width > 1024 || info.height > 1024 {
            return Err(());
        }
        let size = reader.output_buffer_size().filter(|size| *size <= 4 * 1024 * 1024).ok_or(())?;
        let mut data = vec![0; size];
        let info = reader.next_frame(&mut data).map_err(|_| ())?;
        let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
        let channels = info.color_type.samples();
        for p in data[..info.buffer_size()].chunks_exact(channels) {
            rgba.extend_from_slice(&match info.color_type {
                png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
                png::ColorType::Rgb => [p[0], p[1], p[2], 255],
                png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
                png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
                png::ColorType::Indexed => return Err(()),
            });
        }
        image::RgbaImage::from_raw(info.width, info.height, rgba).ok_or(())?
    } else {
        let mut reader =
            image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Jpeg);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(1024);
        limits.max_image_height = Some(1024);
        limits.max_alloc = Some(8 * 1024 * 1024);
        reader.limits(limits);
        reader.decode().map_err(|_| ())?.into_rgba8()
    };
    let small = if rgba.width() > 64 || rgba.height() > 64 {
        image::DynamicImage::ImageRgba8(rgba).thumbnail(64, 64).into_rgba8()
    } else {
        rgba
    };
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [small.width() as usize, small.height() as usize],
        small.as_raw(),
    ))
}
pub(crate) fn paint(ui: &egui::Ui, rect: egui::Rect, url: Option<&str>) -> bool {
    if let Some(url) = url
        && let Avatar::Ready(texture) = get(ui.ctx(), url)
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
    fn decoder_bounds_encoded_bytes_pixels_and_output_texture() {
        assert_eq!(decode(&png(128)).unwrap().size, [64, 1]);
        assert!(decode(&png(1025)).is_err());
        assert!(decode(b"garbage").is_err());
        assert!(decode(&png(16)[..20]).is_err());
        assert!(bounded(std::io::repeat(0)).is_err());
        assert!(decode(&vec![0; MAX_BYTES as usize + 1]).is_err());
    }
    #[test]
    fn included_avatar_uses_the_real_async_decoder_and_is_cached() {
        let ctx = egui::Context::default();
        let url = "https://example.invalid/avatar.png";
        ctx.include_bytes(url, png(16));
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            if let Avatar::Ready(texture) = get(&ctx, url) {
                assert_eq!(texture.size(), [16, 1]);
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(cache(&ctx).lock().unwrap().entries.len(), 1);
    }
}
