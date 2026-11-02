//! GStreamer playback engine — crossfade architecture.
//!
//! Rewritten from the single-`playbin` engine (which could only do gapless
//! hard-cuts via `about-to-finish`) to a **dual `uridecodebin` + shared
//! `audiomixer`** topology, so two tracks can be decoded simultaneously and
//! cross-faded. This is the same architecture Clementine's GStreamer engine
//! uses (see docs/ARCHITECTURE_DECISIONS.md "Crossfade playback design" for
//! the full rationale and the Clementine citation), except Echo keeps ONE DSP
//! chain (Concert Hall/mono/speed) *downstream of the mixer* so both tracks
//! share a single effects path and a single audio sink.
//!
//! Topology:
//! ```text
//! uridecodebin A --pad-added--> audiomixer sink_0 --\
//! uridecodebin B --pad-added--> audiomixer sink_1 ----+--> [DSP chain] --> sink
//! ```
//! Crossfade is done by ramping the two mixer-pad `volume` properties (0..1),
//! driven manually from `poll_bus` on the main thread — the pinned
//! gstreamer 0.25.3 crate does not expose the GstController interpolation API
//! (verified; see ARCHITECTURE_DECISIONS.md), so the ramp is stepped.
//!
//! `crossfade_ms == 0` means gapless: no pre-emptive fade, tracks advance on
//! EOS with a (slightly less precise than the old `about-to-finish`) handoff.
//! A future refinement is the `drained` signal + `gst_pad_set_offset` trick
//! for sample-accurate joins (documented in ARCHITECTURE_DECISIONS.md).
//!
//! DOES NOT include 8D audio (removed by user decision).

use std::path::Path;
use std::time::Instant;

use gst::prelude::*;
use log::{error, info, warn};

use crate::error::PlayerError;

pub struct DspAvailability {
    pub speed: bool,
    pub reverb: bool,
    pub crossfeed: bool,
}

impl DspAvailability {
    /// Concert Hall needs BOTH reverb and crossfeed; if only one exists the
    /// effect is partial. This was previously conflated (reported available
    /// if `reverb || crossfeed`), which the audit correctly flagged.
    pub fn concert_hall(&self) -> bool {
        self.reverb && self.crossfeed
    }
}

/// Which of the two decodebins is currently the "current" track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    A,
    B,
}

impl Side {
    fn other(self) -> Side {
        match self {
            Side::A => Side::B,
            Side::B => Side::A,
        }
    }
}

/// What `poll_bus` reports back to the caller (main.rs), which owns the queue
/// and decides the next track.
#[derive(Debug, PartialEq)]
pub enum PlayerEvent {
    None,
    /// The active track reached EOS with no crossfade in progress — the caller
    /// should advance the queue.
    TrackEnded,
    /// A crossfade just completed: the fading-out track is idle and the
    /// preloaded track is now the current one.
    FadeComplete,
}

pub struct Player {
    pipeline: gst::Pipeline,
    decode_a: gst::Element,
    decode_b: gst::Element,
    mixer_pad_a: gst::Pad,
    mixer_pad_b: gst::Pad,
    pitch_elem: Option<gst::Element>,
    reverb_elem: Option<gst::Element>,
    bs2b_elem: Option<gst::Element>,
    monocaps: gst::Element,
    pub speed: f64,
    pub mono_downmix: bool,
    pub concert_hall_enabled: bool,
    /// Real playback state, driven by bus StateChanged messages (see poll_bus).
    pub playing: bool,
    /// Which decodebin is the current track.
    active: Side,
    /// A crossfade is in progress.
    fading: bool,
    fade_start: Instant,
    /// Session crossfade duration in ms. 0 = gapless (no fade).
    pub crossfade_ms: u64,
    /// 0.0..1.0 progress of the in-flight fade (for the UI).
    pub fade_progress: f32,
}

impl Player {
    pub fn new() -> Result<Self, PlayerError> {
        gst::init()?;

        let pipeline = gst::Pipeline::new();

        let decode_a = gst::ElementFactory::make("uridecodebin").build()?;
        let decode_b = gst::ElementFactory::make("uridecodebin").build()?;
        let mixer = gst::ElementFactory::make("audiomixer").build()?;

        // Shared DSP chain (downstream of the mixer) — identical order to the
        // original playbin engine: convert -> [pitch] -> [reverb] -> [bs2b]
        // -> convert_mono -> monocaps -> convert2 -> spectrum -> sink.
        let spectrum = gst::ElementFactory::make("spectrum")
            .property("bands", 32u32)
            .property("threshold", -70i32)
            // false until something actually drains the bus (see poll_bus)
            .property("post-messages", false)
            .property("interval", 50_000_000u64)
            .build()?;

        let convert1 = gst::ElementFactory::make("audioconvert").build()?;
        let convert_mono = gst::ElementFactory::make("audioconvert").build()?;
        let convert2 = gst::ElementFactory::make("audioconvert").build()?;
        let real_sink = gst::ElementFactory::make("autoaudiosink").build()?;

        let pitch_elem = try_make_element("pitch", "speed/pitch control");
        let monocaps = gst::ElementFactory::make("capsfilter")
            .property("caps", &gst::Caps::builder("audio/x-raw").build())
            .build()?;

        let reverb_elem = try_make_element("freeverb", "Concert Hall reverb");
        if let Some(ref reverb) = reverb_elem {
            reverb.set_property("room-size", 0.97f32);
            reverb.set_property("damping", 0.25f32);
            reverb.set_property("width", 1.0f32);
            reverb.set_property("level", 0.0f32);
        }
        let bs2b_elem = try_make_element("bs2b", "Concert Hall crossfeed");
        if let Some(ref bs2b) = bs2b_elem {
            bs2b.set_property("fcut", 700i32);
            bs2b.set_property("feed", 10i32);
        }

        // Add everything to the pipeline.
        pipeline.add(&decode_a).map_err(e("decode_a"))?;
        pipeline.add(&decode_b).map_err(e("decode_b"))?;
        pipeline.add(&mixer).map_err(e("mixer"))?;
        pipeline.add(&convert1).map_err(e("convert1"))?;
        if let Some(ref p) = pitch_elem {
            pipeline.add(p).map_err(e("pitch"))?;
        }
        if let Some(ref r) = reverb_elem {
            pipeline.add(r).map_err(e("reverb"))?;
        }
        if let Some(ref b) = bs2b_elem {
            pipeline.add(b).map_err(e("bs2b"))?;
        }
        pipeline.add(&convert_mono).map_err(e("convert_mono"))?;
        pipeline.add(&monocaps).map_err(e("monocaps"))?;
        pipeline.add(&convert2).map_err(e("convert2"))?;
        pipeline.add(&spectrum).map_err(e("spectrum"))?;
        pipeline.add(&real_sink).map_err(e("real_sink"))?;

        // Request the two mixer input pads (template "sink_%u").
        let mixer_pad_a = mixer
            .request_pad_simple("sink_%u")
            .ok_or_else(|| PlayerError::Other("audiomixer produced no sink pad for A".into()))?;
        let mixer_pad_b = mixer
            .request_pad_simple("sink_%u")
            .ok_or_else(|| PlayerError::Other("audiomixer produced no sink pad for B".into()))?;
        mixer_pad_a.set_property("volume", 1.0);
        mixer_pad_b.set_property("volume", 0.0);

        // Link the DSP chain: mixer -> convert1 -> [pitch] -> [reverb] ->
        // [bs2b] -> convert_mono -> monocaps -> convert2 -> spectrum -> sink.
        let mut chain: Vec<&gst::Element> = vec![&mixer, &convert1];
        if let Some(ref p) = pitch_elem {
            chain.push(p);
        }
        if let Some(ref r) = reverb_elem {
            chain.push(r);
        }
        if let Some(ref b) = bs2b_elem {
            chain.push(b);
        }
        chain.push(&convert_mono);
        chain.push(&monocaps);
        chain.push(&convert2);
        chain.push(&spectrum);
        chain.push(&real_sink);
        for pair in chain.windows(2) {
            pair[0].link(pair[1]).map_err(|e| {
                PlayerError::Other(format!("failed to link {} -> {}: {e}", pair[0].name(), pair[1].name()))
            })?;
        }

        // Wire each decodebin's audio pads to its mixer input.
        wire_decodebin(&decode_a, mixer_pad_a.clone());
        wire_decodebin(&decode_b, mixer_pad_b.clone());

        info!(
            "crossfade engine built - pitch: {}, freeverb: {}, bs2b: {}",
            pitch_elem.is_some(),
            reverb_elem.is_some(),
            bs2b_elem.is_some()
        );

        Ok(Self {
            pipeline,
            decode_a,
            decode_b,
            mixer_pad_a,
            mixer_pad_b,
            pitch_elem,
            reverb_elem,
            bs2b_elem,
            monocaps,
            speed: 1.0,
            mono_downmix: false,
            concert_hall_enabled: false,
            playing: false,
            active: Side::A,
            fading: false,
            fade_start: Instant::now(),
            crossfade_ms: 0,
            fade_progress: 0.0,
        })
    }

    fn decode(&self, side: Side) -> &gst::Element {
        match side {
            Side::A => &self.decode_a,
            Side::B => &self.decode_b,
        }
    }

    fn set_side_volume(&self, side: Side, vol: f64) {
        let pad = match side {
            Side::A => &self.mixer_pad_a,
            Side::B => &self.mixer_pad_b,
        };
        pad.set_property("volume", vol.clamp(0.0, 1.0));
    }

    pub fn dsp_available(&self) -> DspAvailability {
        DspAvailability {
            speed: self.pitch_elem.is_some(),
            reverb: self.reverb_elem.is_some(),
            crossfeed: self.bs2b_elem.is_some(),
        }
    }

    #[allow(dead_code)]
    pub fn bus(&self) -> Option<gst::Bus> {
        self.pipeline.bus()
    }

    /// Advance any in-flight crossfade, then drain bus messages. Call on the
    /// main-thread timer tick (same cadence as the old `poll_bus`).
    ///
    /// Returns the most important event that happened this tick:
    /// - `FadeComplete` when a crossfade just finished (the preloaded track is
    ///   now current — the caller must flip `current_track` and preload the
    ///   next-next track).
    /// - `TrackEnded` when the active track reached EOS with no fade (advance
    ///   the queue).
    pub fn poll_bus(&mut self) -> PlayerEvent {
        let mut fade_complete = false;

        // Advance the fade (manual stepped ramp — see module doc).
        if self.fading {
            let elapsed = self.fade_start.elapsed().as_millis() as u64;
            let dur = self.crossfade_ms.max(1);
            let p = ((elapsed as f64) / (dur as f64)).min(1.0);
            self.fade_progress = p as f32;
            let (vol_active, vol_idle) = (1.0 - p, p);
            self.set_side_volume(self.active, vol_active);
            self.set_side_volume(self.active.other(), vol_idle);

            if p >= 1.0 {
                let old = self.active;
                let new = old.other();
                // Retire the fading-out track.
                if let Err(e) = self.decode(old).set_state(gst::State::Null) {
                    error!("failed to retire faded-out decodebin: {e}");
                }
                self.set_side_volume(old, 0.0);
                self.set_side_volume(new, 1.0);
                self.active = new;
                self.fading = false;
                self.fade_progress = 0.0;
                fade_complete = true;
            }
        }

        // Drain bus messages.
        let mut track_ended = false;
        if let Some(bus) = self.pipeline.bus() {
            while let Some(msg) = bus.pop() {
                match msg.view() {
                    gst::MessageView::Eos(_) => {
                        // During a fade, the fading-out side reaching EOS is
                        // expected (the fade completes it); ignore it so the
                        // queue doesn't advance twice.
                        if !self.fading {
                            track_ended = true;
                        }
                    }
                    gst::MessageView::Error(err) => {
                        error!(
                            "GStreamer pipeline error from {:?}: {} (debug: {:?})",
                            err.src().map(|s| s.path_string()),
                            err.error(),
                            err.debug()
                        );
                        if !self.fading {
                            track_ended = true;
                        }
                    }
                    gst::MessageView::Warning(w) => {
                        warn!(
                            "GStreamer warning from {:?}: {} (debug: {:?})",
                            w.src().map(|s| s.path_string()),
                            w.error(),
                            w.debug()
                        );
                    }
                    gst::MessageView::StateChanged(state) => {
                        // Only the pipeline's own state (not every child's).
                        // Compare as glib::Object so the Pipeline vs Object
                        // type difference doesn't matter (pointer equality).
                        let is_pipeline = state
                            .src()
                            .map(|src| {
                                let a: glib::Object = src.clone().upcast();
                                let b: glib::Object = self.pipeline.clone().upcast();
                                a == b
                            })
                            .unwrap_or(false);
                        if is_pipeline {
                            self.playing = state.current() == gst::State::Playing;
                        }
                    }
                    _ => (),
                }
            }
        }

        if fade_complete {
            PlayerEvent::FadeComplete
        } else if track_ended {
            PlayerEvent::TrackEnded
        } else {
            PlayerEvent::None
        }
    }

    /// Load a track into the active decodebin and reset to a clean
    /// single-track state (no fade).
    pub fn load_and_play(&mut self, path: &Path) -> Result<(), PlayerError> {
        self.pipeline.set_state(gst::State::Null)?;
        let uri = path_to_uri(path)?;
        self.decode_a.set_property("uri", uri.as_str());
        // B must have SOME valid uri for the pipeline's Playing transition to
        // preroll (an idle uridecodebin with no uri errors the whole pipeline
        // - verified: "No URI specified to play from"). Mirror A's uri; B
        // decodes the same track silently (volume 0) during normal playback,
        // then re-decodes the *next* track when a crossfade starts.
        self.decode_b.set_property("uri", uri.as_str());
        self.set_side_volume(Side::A, 1.0);
        self.set_side_volume(Side::B, 0.0);
        self.active = Side::A;
        self.fading = false;
        self.fade_progress = 0.0;
        self.pipeline.set_state(gst::State::Playing)?;
        Ok(())
    }

    /// Start crossfading from the current track into `next`. The idle
    /// decodebin is pointed at `next` and set Playing; the fade ramp begins
    /// on the next `poll_bus` tick.
    pub fn begin_crossfade(&mut self, next: &Path) -> Result<(), PlayerError> {
        let idle = self.active.other();
        let uri = path_to_uri(next)?;
        self.decode(idle).set_property("uri", uri.as_str());
        self.decode(idle).set_state(gst::State::Playing)?;
        self.set_side_volume(idle, 0.0);
        self.fading = true;
        self.fade_start = Instant::now();
        self.fade_progress = 0.0;
        Ok(())
    }

    /// True when the active track is within `crossfade_ms` of its end (so the
    /// caller can pre-emptively begin a crossfade). Always false when
    /// `crossfade_ms == 0` (gapless) or already fading.
    pub fn near_end(&self) -> bool {
        if self.fading || self.crossfade_ms == 0 {
            return false;
        }
        match (self.position_ms(), self.duration_ms()) {
            (Some(pos), Some(dur)) if dur > 0 => pos + self.crossfade_ms >= dur,
            _ => false,
        }
    }

    pub fn set_crossfade_ms(&mut self, ms: u64) {
        self.crossfade_ms = ms;
    }

    pub fn play(&self) -> Result<(), PlayerError> {
        self.pipeline.set_state(gst::State::Playing)?;
        Ok(())
    }

    pub fn pause(&self) -> Result<(), PlayerError> {
        self.pipeline.set_state(gst::State::Paused)?;
        Ok(())
    }

    pub fn stop(&self) -> Result<(), PlayerError> {
        self.pipeline.set_state(gst::State::Null)?;
        Ok(())
    }

    pub fn seek(&self, position_ms: u64) -> Result<(), PlayerError> {
        self.pipeline
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::ClockTime::from_mseconds(position_ms),
            )
            .map_err(|e| PlayerError::Other(format!("seek failed: {e}")))
    }

    #[allow(dead_code)]
    pub fn set_volume(&self, volume: f64) {
        // No dedicated volume element anymore; leave a no-op for API
        // compatibility (mixer pad volumes are managed by the crossfader).
        let _ = volume;
    }

    pub fn position_ms(&self) -> Option<u64> {
        self.pipeline
            .query_position::<gst::ClockTime>()
            .map(|t| t.mseconds())
    }

    pub fn duration_ms(&self) -> Option<u64> {
        self.pipeline
            .query_duration::<gst::ClockTime>()
            .map(|t| t.mseconds())
    }

    pub fn set_speed(&mut self, tempo: f64) {
        self.speed = tempo.clamp(0.5, 2.0);
        if let Some(ref pitch) = self.pitch_elem {
            // GstPitch `tempo` is a gfloat (f32), not gdouble — f64 panics.
            pitch.set_property("tempo", self.speed as f32);
        } else {
            warn!("set_speed called but pitch element unavailable");
        }
    }

    pub fn set_mono_downmix(&mut self, enabled: bool) {
        self.mono_downmix = enabled;
        let caps = if enabled {
            gst::Caps::builder("audio/x-raw").field("channels", 1i32).build()
        } else {
            gst::Caps::builder("audio/x-raw").build()
        };
        self.monocaps.set_property("caps", &caps);
        // Force renegotiation (same real bug as the Python version — see
        // ARCHITECTURE_DECISIONS.md).
        if let Some(pos) = self.position_ms() {
            if let Err(e) = self.seek(pos) {
                error!("mono downmix renegotiation seek failed: {e}");
            }
        }
    }

    pub fn set_concert_hall(&mut self, enabled: bool) {
        self.concert_hall_enabled = enabled;
        if let Some(ref reverb) = self.reverb_elem {
            reverb.set_property("level", if enabled { 0.97f32 } else { 0.0f32 });
        }
        if let Some(ref bs2b) = self.bs2b_elem {
            bs2b.set_property("feed", if enabled { 60i32 } else { 10i32 });
        }
    }
}

/// Wire a `uridecodebin`'s audio pads to the given mixer input pad. Only audio
/// pads are linked (video/subtitle pads are ignored).
fn wire_decodebin(decode: &gst::Element, mixer_pad: gst::Pad) {
    let mixer_pad_for_closure = mixer_pad.clone();
    decode.connect("pad-added", false, move |args| {
        // glib-rs `connect` passes the emitter as args[0] and the signal's
        // first argument (the new pad) as args[1].
        let pad = args.get(1).and_then(|v| v.get::<gst::Pad>().ok());
        if let Some(pad) = pad {
            let is_audio = pad
                .current_caps()
                .map(|c| c.iter().any(|s| s.name().starts_with("audio/")))
                .unwrap_or(true);
            if is_audio {
                if let Err(e) = pad.link(&mixer_pad_for_closure) {
                    warn!("failed to link decodebin pad to mixer: {e}");
                }
            }
        }
        None
    });
}

fn e(what: &str) -> impl Fn(glib::BoolError) -> PlayerError + '_ {
    move |err| PlayerError::Other(format!("failed to add {what} to pipeline: {err}"))
}

fn try_make_element(factory_name: &str, human_description: &str) -> Option<gst::Element> {
    match gst::ElementFactory::make(factory_name).build() {
        Ok(el) => Some(el),
        Err(e) => {
            warn!(
                "optional element '{factory_name}' ({human_description}) unavailable, \
                 that feature will no-op: {e}"
            );
            None
        }
    }
}

/// Convert a filesystem path to a `file://` URI. `glib::filename_to_uri`, not
/// a gst wrapper (see ARCHITECTURE_DECISIONS.md).
pub fn path_to_uri(path: &Path) -> Result<String, PlayerError> {
    glib::filename_to_uri(path, None)
        .map(|uri| uri.to_string())
        .map_err(|e| PlayerError::Other(format!("bad file path {}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::{path_to_uri, Player, PlayerEvent};

    fn write_test_wav_secs(path: &std::path::Path, secs: u32) {
        let sample_rate: u32 = 8000;
        let data_len = sample_rate * secs;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVE");
        b.extend_from_slice(b"fmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&sample_rate.to_le_bytes());
        b.extend_from_slice(&sample_rate.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&8u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        b.extend(std::iter::repeat(128u8).take(data_len as usize));
        std::fs::write(path, b).unwrap();
    }

    #[test]
    fn path_to_uri_is_file_uri() {
        let uri = path_to_uri(std::path::Path::new("/tmp/x y.mp3")).unwrap();
        assert!(uri.starts_with("file://"));
        assert!(uri.contains("x%20y"));
    }

    /// Regression: `poll_bus` must actually see bus messages (the old
    /// `add_signal_watch` consumed them, leaving `playing` false forever).
    #[test]
    fn playing_state_tracks_pipeline() {
        let dir = std::env::temp_dir().join(format!("echo_xfade_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("tone.wav");
        write_test_wav_secs(&wav, 1);

        let mut player = Player::new().unwrap();
        player.load_and_play(&wav).unwrap();

        let mut became_playing = false;
        for _ in 0..100 {
            player.poll_bus();
            if player.playing {
                became_playing = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(became_playing, "pipeline should report Playing after load_and_play");

        player.stop().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Regression: enabling mono used to force `channels=1` on a stereo stream
    /// with no converter, failing negotiation and erroring through every track.
    #[test]
    fn mono_downmix_does_not_break_playback() {
        let dir = std::env::temp_dir().join(format!("echo_xfade_mono_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("tone.wav");
        write_test_wav_secs(&wav, 4);

        let mut player = Player::new().unwrap();
        player.load_and_play(&wav).unwrap();

        let mut playing = false;
        for _ in 0..100 {
            player.poll_bus();
            if player.playing {
                playing = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(playing, "should be playing before toggling mono");

        player.set_mono_downmix(true);
        let mut errored = false;
        for _ in 0..20 {
            if player.poll_bus() != PlayerEvent::None {
                errored = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(!errored, "mono downmix must not error the pipeline");

        player.stop().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A short track with a long crossfade should produce a FadeComplete once
    /// the ramp reaches 1.0, without needing real EOS timing.
    #[test]
    fn crossfade_completes_fade() {
        let dir = std::env::temp_dir().join(format!("echo_xfade_fade_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.wav");
        let b = dir.join("b.wav");
        write_test_wav_secs(&a, 4);
        write_test_wav_secs(&b, 4);

        let mut player = Player::new().unwrap();
        player.load_and_play(&a).unwrap();
        player.set_crossfade_ms(200);

        // Force a transition immediately (simulating the main thread's
        // near_end() trigger) and let the fade run to completion.
        player.begin_crossfade(&b).unwrap();
        assert!(player.fading);

        let mut completed = false;
        for _ in 0..40 {
            if player.poll_bus() == PlayerEvent::FadeComplete {
                completed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(completed, "200ms crossfade should complete well within 2s");
        assert!(!player.fading);

        player.stop().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
