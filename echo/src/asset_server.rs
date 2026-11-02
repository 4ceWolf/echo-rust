//! `echo://` custom-protocol asset server.
//!
//! The webview cannot read files from disk, so album art and depth masks
//! (which the backend only ever exposes as *paths* - see `UiState`) are
//! served to the page over a wry custom protocol. Two routes:
//!
//! - `echo://art/?path=<percent-encoded track path>` -> the front cover for
//!   that track (embedded tag art, or a sidecar image via `art.rs`).
//! - `echo://mask/?path=<percent-encoded mask path>` -> the cached depth
//!   mask PNG (white = foreground).
//!
//! Optional `w`/`h` query parameters request a display-sized, aspect-correct
//! ("cover") crop: the art is resized to exactly `w`x`h` and center-cropped,
//! preserving its aspect ratio (never stretched). The page measures the real
//! display area and asks for that shape, so a square slot gets square art and
//! a rectangular slot gets rectangular art. Resized results are cached in
//! memory for the process ("per session"), keyed by route+path+size, so a
//! window resize or repeated render does not re-decode the source image.
//!
//! This is the UI-session decision flagged in GUI_SESSION.md ("the webview
//! can't read files from disk"). It keeps the Rust<->JS state contract
//! unchanged: state still carries paths, and the page turns a path into a
//! URL with `encodeURIComponent`.
//!
//! Responses are `Access-Control-Allow-Origin: *` so the page can draw the
//! art into a canvas and extract a dynamic accent without tainting it.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView};

use crate::art;

/// The custom scheme name registered with wry. Keep in sync with the
/// `echo://` URLs the frontend builds.
pub const SCHEME: &str = "echo";

/// Largest edge the resize endpoint will produce, so a malformed request
/// can't ask for a gigapixel allocation.
const MAX_EDGE: u32 = 2048;

/// JPEG quality for resized art (the mask is always lossless PNG).
const ART_JPEG_QUALITY: u8 = 86;

type AssetResponse = wry::http::Response<Cow<'static, [u8]>>;

/// Per-session resize cache: cache key -> (mime, encoded bytes).
fn resize_cache() -> &'static Mutex<HashMap<String, (String, Vec<u8>)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (String, Vec<u8>)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Session cache of *source* artwork (mime + raw bytes) keyed by track path.
/// Without this, every thumbnail request re-reads the file and re-parses its
/// tags with `lofty` - the main cause of Browse scroll lag.
fn source_cache() -> &'static Mutex<HashMap<String, (String, Vec<u8>)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (String, Vec<u8>)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

struct AssetJob {
    request: wry::http::Request<Vec<u8>>,
    responder: wry::RequestAsyncResponder,
}

/// A small fixed worker pool for the asset protocol. Decoding/resizing art on
/// the GTK main thread stalls the whole UI when Browse requests many covers at
/// once, so requests are handed to these workers and answered asynchronously.
fn worker_pool() -> &'static [mpsc::Sender<AssetJob>] {
    static POOL: OnceLock<Vec<mpsc::Sender<AssetJob>>> = OnceLock::new();
    POOL.get_or_init(|| {
        let count = std::thread::available_parallelism()
            .map(|n| n.get().clamp(2, 4))
            .unwrap_or(4);
        (0..count)
            .map(|_| {
                let (tx, rx) = mpsc::channel::<AssetJob>();
                let _ = std::thread::Builder::new()
                    .name("echo-asset-worker".into())
                    .spawn(move || {
                        for job in rx {
                            job.responder.respond(serve("", job.request));
                        }
                    });
                tx
            })
            .collect()
    })
}

/// Async entry point registered with wry (`with_asynchronous_custom_protocol`).
pub fn serve_async(
    _id: wry::WebViewId,
    request: wry::http::Request<Vec<u8>>,
    responder: wry::RequestAsyncResponder,
) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let pool = worker_pool();
    let idx = NEXT.fetch_add(1, Ordering::Relaxed) % pool.len().max(1);
    let _ = pool[idx].send(AssetJob { request, responder });
}

/// wry custom-protocol entry point. Matches on the URL host (`art`/`mask`)
/// and percent-decodes the `path` query parameter.
pub fn serve(
    _id: wry::WebViewId,
    request: wry::http::Request<Vec<u8>>,
) -> AssetResponse {
    let uri = request.uri();
    let host = uri.host().unwrap_or("");
    // Bundled sound cues (e.g. the light-mode flashbang) are served by name.
    if host == "sfx" {
        return serve_sfx(uri.path().trim_start_matches('/'));
    }
    let query = uri.query().unwrap_or("");
    let Some(path) = query_value(query, "path").map(|p| percent_decode(&p)) else {
        return not_found();
    };

    let size = match (
        query_value(query, "w").and_then(|v| v.parse::<u32>().ok()),
        query_value(query, "h").and_then(|v| v.parse::<u32>().ok()),
    ) {
        (Some(w), Some(h)) if w > 0 && h > 0 => Some((w.min(MAX_EDGE), h.min(MAX_EDGE))),
        _ => None,
    };

    match host {
        "art" => serve_art(Path::new(&path), size),
        "mask" => serve_mask(Path::new(&path), size),
        "subject" => {
            let mask = query_value(query, "mask").map(|p| percent_decode(&p));
            match mask {
                Some(m) => serve_subject(Path::new(&path), Path::new(&m), size),
                None => not_found(),
            }
        }
        "lrc" => serve_lrc(Path::new(&path)),
        _ => not_found(),
    }
}

/// Bundled sound cue for the dark->light flashbang transition.
static FLASHBANG: &[u8] = include_bytes!("../assets/sfx/flashbang.mp3");

fn serve_sfx(name: &str) -> AssetResponse {
    match name {
        "flashbang" => bytes_response("audio/mpeg", FLASHBANG.to_vec()),
        _ => not_found(),
    }
}

/// Serve a `.lrc` lyrics file as UTF-8 text. Restricted to `.lrc` so the
/// custom protocol can't be used to read arbitrary local files from the page.
fn serve_lrc(path: &Path) -> AssetResponse {
    let is_lrc = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("lrc"))
        .unwrap_or(false);
    if !is_lrc {
        return not_found();
    }
    match std::fs::read(path) {
        Ok(data) => bytes_response("text/plain; charset=utf-8", data),
        Err(e) => {
            log::warn!("asset_server: lrc read failed for {}: {e}", path.display());
            not_found()
        }
    }
}

fn serve_art(path: &Path, size: Option<(u32, u32)>) -> AssetResponse {
    let Some((mime, data)) = art_source(path) else {
        return not_found();
    };
    match size {
        Some((w, h)) => resize_asset("art", path, data, w, h, true),
        None => bytes_response(&mime, data),
    }
}

/// Embedded/sidecar art bytes for a track, cached for the session.
fn art_source(path: &Path) -> Option<(String, Vec<u8>)> {
    let key = path.to_string_lossy().to_string();
    if let Some(v) = source_cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return Some(v);
    }
    match art::extract_front_cover(path) {
        Ok(Some(artwork)) => {
            let entry = (artwork.mime, artwork.data);
            if let Ok(mut cache) = source_cache().lock() {
                if cache.len() >= 512 {
                    cache.clear();
                }
                cache.insert(key, entry.clone());
            }
            Some(entry)
        }
        Ok(None) => None,
        Err(e) => {
            log::warn!("asset_server: art read failed for {}: {e}", path.display());
            None
        }
    }
}

/// Compose the depth "subject": album art with the depth mask's luminance as
/// its alpha channel, producing a real transparent cutout. This is done here
/// because WebKitGTK does not support CSS luminance masks (it falls back to
/// alpha, so the grayscale mask renders fully opaque - which looked like a
/// second, misaligned copy of the art instead of a depth subject).
fn serve_subject(art_path: &Path, mask_path: &Path, size: Option<(u32, u32)>) -> AssetResponse {
    let key = format!(
        "subject|{}|{}|{:?}",
        art_path.display(),
        mask_path.display(),
        size
    );
    if let Some((mime, bytes)) = resize_cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return bytes_response(&mime, bytes);
    }

    let Some((_, art_data)) = art_source(art_path) else {
        return not_found();
    };
    let mask_data = match std::fs::read(mask_path) {
        Ok(d) => d,
        Err(e) => {
            log::warn!(
                "asset_server: subject mask read failed for {}: {e}",
                mask_path.display()
            );
            return not_found();
        }
    };

    let (art, mask) = match (
        image::load_from_memory(&art_data),
        image::load_from_memory(&mask_data),
    ) {
        (Ok(a), Ok(m)) => (a, m),
        _ => return not_found(),
    };

    let (aw, ah) = art.dimensions();
    let mask = if mask.dimensions() != (aw, ah) {
        mask.resize_exact(aw, ah, FilterType::Triangle)
    } else {
        mask
    };

    let (art_c, mask_c) = match size {
        Some((w, h)) => (cover_crop(&art, w, h), cover_crop(&mask, w, h)),
        None => (art, mask),
    };

    let (w, h) = art_c.dimensions();
    let art_rgba = art_c.to_rgba8();
    let mask_l = mask_c.to_luma8();
    let mut out = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let p = art_rgba.get_pixel(x, y);
            let a = mask_l.get_pixel(x, y)[0];
            out.put_pixel(x, y, image::Rgba([p[0], p[1], p[2], a]));
        }
    }

    let Some(png) = encode_png(&DynamicImage::ImageRgba8(out)) else {
        return not_found();
    };
    if let Ok(mut cache) = resize_cache().lock() {
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert(key, ("image/png".to_string(), png.clone()));
    }
    bytes_response("image/png", png)
}

fn serve_mask(path: &Path, size: Option<(u32, u32)>) -> AssetResponse {
    match std::fs::read(path) {
        Ok(data) => match size {
            Some((w, h)) => resize_asset("mask", path, data, w, h, false),
            None => bytes_response("image/png", data),
        },
        Err(e) => {
            log::warn!("asset_server: mask read failed for {}: {e}", path.display());
            not_found()
        }
    }
}

/// Decode `data`, cover-crop it to exactly `w`x`h`, encode, and cache it for
/// the session. `to_jpeg` picks the output codec (art) vs lossless PNG
/// (depth mask); both art and mask use the same crop math, so a mask and its
/// art stay pixel-aligned in the same display slot.
fn resize_asset(
    kind: &str,
    src_path: &Path,
    data: Vec<u8>,
    w: u32,
    h: u32,
    to_jpeg: bool,
) -> AssetResponse {
    let key = format!("{kind}|{}|{w}x{h}", src_path.display());
    if let Some((mime, bytes)) = resize_cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return bytes_response(&mime, bytes);
    }

    let img = match image::load_from_memory(&data) {
        Ok(img) => img,
        Err(e) => {
            log::warn!(
                "asset_server: failed to decode {} for resize: {e}",
                src_path.display()
            );
            return not_found();
        }
    };

    let cropped = cover_crop(&img, w, h);
    let encoded = if to_jpeg {
        encode_jpeg(&cropped, ART_JPEG_QUALITY).map(|b| ("image/jpeg", b))
    } else {
        encode_png(&cropped).map(|b| ("image/png", b))
    };

    let Some((mime, bytes)) = encoded else {
        return not_found();
    };

    if let Ok(mut cache) = resize_cache().lock() {
        // Crude session cap: art sizes are few, so this rarely trips.
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert(key, (mime.to_string(), bytes.clone()));
    }
    bytes_response(mime, bytes)
}

/// Resize `img` to cover `w`x`h` (scale so both dimensions are >= target,
/// preserving aspect ratio) and center-crop the overflow. Never stretches.
fn cover_crop(img: &DynamicImage, w: u32, h: u32) -> DynamicImage {
    let (sw, sh) = img.dimensions();
    if sw == 0 || sh == 0 {
        return img.clone();
    }
    if sw == w && sh == h {
        return img.clone();
    }
    let scale = (w as f32 / sw as f32).max(h as f32 / sh as f32);
    let nw = ((sw as f32 * scale).ceil() as u32).max(w).max(1);
    let nh = ((sh as f32 * scale).ceil() as u32).max(h).max(1);
    let resized = img.resize_exact(nw, nh, FilterType::Lanczos3);
    let x = (nw - w) / 2;
    let y = (nh - h) / 2;
    resized.crop_imm(x, y, w, h)
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Option<Vec<u8>> {
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut std::io::Cursor::new(&mut out), quality)
        .encode_image(&rgb)
        .ok()?;
    Some(out)
}

fn encode_png(img: &DynamicImage) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .ok()?;
    Some(out)
}

fn bytes_response(mime: &str, data: Vec<u8>) -> AssetResponse {
    wry::http::Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "no-cache")
        .body(Cow::Owned(data))
        .unwrap_or_else(|_| not_found())
}

fn not_found() -> AssetResponse {
    wry::http::Response::builder()
        .status(404)
        .header("Access-Control-Allow-Origin", "*")
        .body(Cow::Borrowed(&b""[..]))
        .expect("static 404 response is valid")
}

/// Pull one `key=value` pair out of a `&`-separated query string.
fn query_value(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| v.to_string())
    })
}

/// Minimal percent-decoder (no `percent-encoding` dependency): decodes `%XX`
/// and `+`, leaves everything else untouched. Invalid escapes pass through.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                    out.push((hi << 4) | lo);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{cover_crop, percent_decode, query_value};
    use image::{DynamicImage, GenericImageView, Rgb, RgbImage};

    fn decode_size(resp: &wry::http::Response<std::borrow::Cow<'static, [u8]>>) -> (u32, u32) {
        let img = image::load_from_memory(resp.body().as_ref()).unwrap();
        img.dimensions()
    }

    #[test]
    fn percent_decode_handles_escapes_and_spaces() {
        assert_eq!(percent_decode("/music/a%20b.mp3"), "/music/a b.mp3");
        assert_eq!(percent_decode("%2Fhome%2Fkyo"), "/home/user");
        assert_eq!(percent_decode("no+space"), "no space");
        assert_eq!(percent_decode("100%"), "100%"); // dangling % passes through
    }

    #[test]
    fn query_value_finds_path() {
        assert_eq!(
            query_value("path=%2Fa.mp3&x=1", "path").as_deref(),
            Some("%2Fa.mp3")
        );
        assert_eq!(query_value("x=1", "path"), None);
    }

    #[test]
    fn cover_crop_preserves_aspect_without_stretching() {
        // 100x50 source -> square 40x40: scale 0.4 -> 40x20, then crop.
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(100, 50, Rgb([200, 100, 50])));
        assert_eq!(cover_crop(&img, 40, 40).dimensions(), (40, 40));
        // -> wide 80x20: scale max(0.8, 0.4) = 0.8 -> 80x40, crop to 80x20.
        assert_eq!(cover_crop(&img, 80, 20).dimensions(), (80, 20));
        // -> tall 20x80: scale max(0.2, 1.6) = 1.6 -> 160x80, crop to 20x80.
        assert_eq!(cover_crop(&img, 20, 80).dimensions(), (20, 80));
    }

    #[test]
    fn resize_endpoint_returns_requested_square() {
        let dir = std::env::temp_dir().join(format!("echo_asset_rsz_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mask = dir.join("m.png");
        DynamicImage::ImageRgb8(RgbImage::from_pixel(100, 50, Rgb([10, 20, 30])))
            .save(&mask)
            .unwrap();

        let encoded = mask.to_string_lossy().replace('/', "%2F");
        let uri = format!("echo://mask/?path={encoded}&w=40&h=40");
        let req = wry::http::Request::builder()
            .uri(uri)
            .body(Vec::new())
            .unwrap();
        let resp = super::serve("", req);
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.headers().get("Content-Type").unwrap(), "image/png");
        assert_eq!(decode_size(&resp), (40, 40));

        // A second, rectangular request returns the requested shape.
        let uri = format!("echo://mask/?path={encoded}&w=80&h=20");
        let req = wry::http::Request::builder()
            .uri(uri)
            .body(Vec::new())
            .unwrap();
        let resp = super::serve("", req);
        assert_eq!(decode_size(&resp), (80, 20));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn serve_mask_route_reads_file_and_sets_mime() {
        let dir = std::env::temp_dir().join(format!("echo_asset_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mask = dir.join("m.png");
        let bytes = b"\x89PNG\r\n\x1a\nfake-mask";
        std::fs::write(&mask, bytes).unwrap();

        let encoded = mask.to_string_lossy().replace('/', "%2F");
        let uri = format!("echo://mask/?path={encoded}");
        let req = wry::http::Request::builder()
            .uri(uri)
            .body(Vec::new())
            .unwrap();
        let resp = super::serve("", req);
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.headers().get("Content-Type").unwrap(), "image/png");
        assert_eq!(resp.body().as_ref(), bytes);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn serve_unknown_route_is_404() {
        let req = wry::http::Request::builder()
            .uri("echo://nope/?path=%2Fa")
            .body(Vec::new())
            .unwrap();
        assert_eq!(super::serve("", req).status(), 404);
    }

    #[test]
    fn subject_route_uses_mask_luminance_as_alpha() {
        use image::{GenericImage, Rgb, RgbImage, Luma};
        let dir = std::env::temp_dir().join(format!("echo_subject_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // A "track" whose artwork comes from the sidecar cover.png.
        let track = dir.join("song.mp3");
        std::fs::write(&track, b"not audio").unwrap();
        RgbImage::from_pixel(4, 4, Rgb([220, 30, 30])).save(dir.join("cover.png")).unwrap();

        // Mask: left half black (transparent), right half white (opaque).
        let mut mask = image::GrayImage::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                mask.put_pixel(x, y, Luma([if x < 2 { 0 } else { 255 }]));
            }
        }
        mask.save(dir.join("mask.png")).unwrap();

        let uri = format!(
            "echo://subject/?path={}&mask={}&w=4&h=4",
            track.to_string_lossy().replace('/', "%2F"),
            dir.join("mask.png").to_string_lossy().replace('/', "%2F"),
        );
        let req = wry::http::Request::builder().uri(uri).body(Vec::new()).unwrap();
        let resp = super::serve("", req);
        assert_eq!(resp.status(), 200);
        let img = image::load_from_memory(resp.body().as_ref()).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(0, 0).0[3], 0, "mask black -> transparent");
        assert_eq!(img.get_pixel(3, 0).0[3], 255, "mask white -> opaque");
        assert_eq!(img.get_pixel(3, 0).0[0], 220, "art rgb preserved");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn serve_lrc_route_returns_text_and_rejects_other_extensions() {
        let dir = std::env::temp_dir().join(format!("echo_asset_lrc_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lrc = dir.join("song.lrc");
        std::fs::write(&lrc, b"[00:01.00]hi").unwrap();
        let txt = dir.join("song.txt");
        std::fs::write(&txt, b"secret").unwrap();

        let uri = format!("echo://lrc/?path={}", lrc.to_string_lossy().replace('/', "%2F"));
        let req = wry::http::Request::builder().uri(uri).body(Vec::new()).unwrap();
        let resp = super::serve("", req);
        assert_eq!(resp.status(), 200);
        assert!(resp.headers().get("Content-Type").unwrap().to_str().unwrap().starts_with("text/plain"));
        assert_eq!(resp.body().as_ref(), b"[00:01.00]hi");

        let uri = format!("echo://lrc/?path={}", txt.to_string_lossy().replace('/', "%2F"));
        let req = wry::http::Request::builder().uri(uri).body(Vec::new()).unwrap();
        assert_eq!(super::serve("", req).status(), 404);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
