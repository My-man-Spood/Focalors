//! Short SFX for live-game feedback (move, capture, game end).
//! See `assets/sound/` and `CREDITS.md` for provenance/license.
//!
//! Mirrors lichess's own default theme: no distinct "check" sound (it's
//! silent there too — the move/capture sound alone covers it), and every
//! game-ending outcome shares one generic notify sound.

use rodio::{Decoder, Source};
use std::io::Cursor;

/// Default SFX volume (0.0..=1.0). The source files play uncomfortably
/// loud at 1.0; also the fallback used before any Settings value is saved.
pub const DEFAULT_VOLUME: f32 = 0.2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sfx {
    Move,
    Capture,
    GenericNotify,
}

impl Sfx {
    fn bytes(self) -> &'static [u8] {
        match self {
            Sfx::Move => include_bytes!("../assets/sound/Move.mp3"),
            Sfx::Capture => include_bytes!("../assets/sound/Capture.mp3"),
            Sfx::GenericNotify => include_bytes!("../assets/sound/GenericNotify.mp3"),
        }
    }
}

/// Holds the open audio device for as long as the app runs; dropping it
/// stops playback, so this must live on `FocalorsApp` itself, not be
/// reopened per-sound.
pub struct AudioOutput {
    sink: rodio::MixerDeviceSink,
}

impl AudioOutput {
    /// Opens the default output device. `None` on failure (e.g. no audio
    /// device present) so a missing sound card disables sound rather than
    /// crashing the app.
    pub fn open() -> Option<Self> {
        match rodio::DeviceSinkBuilder::open_default_sink() {
            Ok(sink) => Some(Self { sink }),
            Err(e) => {
                eprintln!("Warning: could not open audio output ({e}); sound effects disabled.");
                None
            }
        }
    }

    pub fn play(&self, sfx: Sfx, volume: f32) {
        match Decoder::try_from(Cursor::new(sfx.bytes())) {
            Ok(source) => self.sink.mixer().add(source.amplify(volume.clamp(0.0, 1.0))),
            Err(e) => eprintln!("Warning: could not decode {sfx:?} sound effect: {e}"),
        }
    }
}
