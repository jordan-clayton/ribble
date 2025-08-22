use crate::utils::errors::RibbleError;
use ribble_whisper::audio::pcm::PcmS16Convertible;
use ribble_whisper::audio::recorder::RecorderSample;
use ribble_whisper::transcriber::vad::{
    Earshot, Resettable, Silero,
    SileroBuilder, SileroSampleRate, WebRtc, WebRtcBuilder, WebRtcFilterAggressiveness,
    WebRtcFrameLengthMillis, WebRtcSampleRate, DEFAULT_VOICE_PROPORTION_THRESHOLD, OFFLINE_VOICE_PROBABILITY_THRESHOLD, REAL_TIME_VOICE_PROBABILITY_THRESHOLD,
    VAD,
};
use strum::{AsRefStr, Display, EnumIter, IntoStaticStr};

// NOTE: SILERO V5 STRUGGLES -heavily- WITH LOW-VOLUME AND NOISY SIGNALS
// FOR THE INTERIM, USE WEBRTC OR EARSHOT AS DEFAULT AND UPDATE THE TOOLTIP.

// Silero (v5) can be extremely picky with voice when the signal is poor.
const FLEXIBLE_SILERO_PROBABILITY_THRESHOLD: f32 = 0.15;
const REAL_TIME_VOICED_PROPORTION_THRESHOLD: f32 = 0.4;

// TODO: determine a decent-enough threshold value.
const WEBRTC_HIGH_PROPORTION_THRESHOLD: f32 = 0.7;
const WEBRTC_STRICTEST_PROPORTION_THRESHOLD: f32 = 0.8;

#[derive(Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct VadConfigs {
    vad_type: VadType,
    frame_size: VadFrameSize,
    strictness: VadStrictness,
    // TODO: possibly expose an enum for VoicedProportionThreshold
    use_vad_offline: bool,
}

impl VadConfigs {
    pub(crate) fn new() -> Self {
        Self {
            vad_type: VadType::Auto,
            frame_size: VadFrameSize::Auto,
            strictness: VadStrictness::Auto,
            use_vad_offline: true,
        }
    }

    pub(crate) fn with_vad_type(mut self, vad_type: VadType) -> Self {
        self.vad_type = vad_type;
        self
    }

    pub(crate) fn with_frame_size(mut self, frame_size: VadFrameSize) -> Self {
        self.frame_size = frame_size;
        self
    }

    pub(crate) fn with_strictness(mut self, strictness: VadStrictness) -> Self {
        self.strictness = strictness;
        self
    }
    pub(crate) fn with_use_vad_offline(mut self, use_vad: bool) -> Self {
        self.use_vad_offline = use_vad;
        self
    }

    pub(crate) fn vad_type(&self) -> VadType {
        self.vad_type
    }

    pub(crate) fn frame_size(&self) -> VadFrameSize {
        self.frame_size
    }
    pub(crate) fn strictness(&self) -> VadStrictness {
        self.strictness
    }

    pub(crate) fn use_vad_offline(&self) -> bool {
        self.use_vad_offline
    }


    // Frame size, Aggressiveness, Probability
    fn prep_webrtc(&self) -> (WebRtcFrameLengthMillis, WebRtcFilterAggressiveness, f32) {
        let frame_size = match self.frame_size() {
            VadFrameSize::Small => WebRtcFrameLengthMillis::MS10,
            VadFrameSize::Medium => WebRtcFrameLengthMillis::MS20,
            VadFrameSize::Auto | VadFrameSize::Large => WebRtcFrameLengthMillis::MS30,
        };

        // TODO: keep testing to see what works well.
        // If the aggressiveness remains constant after testing, factor out.
        // Setting the proportion seems to -significantly- improve the accuracy.
        let (aggressiveness, voice_proportion) = match self.strictness() {
            VadStrictness::Flexible => (
                WebRtcFilterAggressiveness::VeryAggressive,
                DEFAULT_VOICE_PROPORTION_THRESHOLD,
            ),
            VadStrictness::Auto | VadStrictness::Medium => (
                WebRtcFilterAggressiveness::VeryAggressive,
                WEBRTC_HIGH_PROPORTION_THRESHOLD,
            ),
            VadStrictness::Strict => (
                WebRtcFilterAggressiveness::VeryAggressive,
                WEBRTC_STRICTEST_PROPORTION_THRESHOLD,
            ),
        };

        (frame_size, aggressiveness, voice_proportion)
    }

    pub(crate) fn build_ribble_vad(&self) -> Result<RibbleVAD, RibbleError> {
        match self.vad_type() {
            VadType::Silero => Ok(RibbleVAD::Silero(self.build_silero()?)),
            VadType::Auto | VadType::WebRtc => Ok(RibbleVAD::WebRtc(self.build_webrtc()?)),
            // VadType::Earshot => Ok(RibbleVAD::Earshot(Box::from(self.build_earshot()?))),
        }
    }

    // These methods leak the abstraction a bit, but are necessary for pushing the dispatch up
    // higher and avoids the need for the type-erasure.
    pub(crate) fn build_silero(&self) -> Result<Silero, RibbleError> {
        if !matches!(self.vad_type, VadType::Silero | VadType::Auto) {
            return Err(RibbleError::Core(format!(
                "Vad type mismatch, cannot build Silero using: {}",
                self.vad_type.as_ref()
            )));
        }

        let probability = match self.strictness() {
            VadStrictness::Auto | VadStrictness::Flexible => FLEXIBLE_SILERO_PROBABILITY_THRESHOLD,
            VadStrictness::Medium => REAL_TIME_VOICE_PROBABILITY_THRESHOLD,
            VadStrictness::Strict => OFFLINE_VOICE_PROBABILITY_THRESHOLD,
        };

        Ok(SileroBuilder::new()
            .with_sample_rate(SileroSampleRate::R16kHz)
            .with_detection_probability_threshold(probability)
            // This might need to be tweaked around, or bump the gain settings.
            .with_voiced_proportion_threshold(REAL_TIME_VOICED_PROPORTION_THRESHOLD)
            .build()?)
    }

    pub(crate) fn build_auto(&self) -> Result<WebRtc, RibbleError> {
        self.build_webrtc()
    }

    pub(crate) fn build_webrtc(&self) -> Result<WebRtc, RibbleError> {
        if !matches!(self.vad_type, VadType::WebRtc | VadType::Auto) {
            return Err(RibbleError::Core(format!(
                "Vad type mismatch, cannot build WebRtc using: {}",
                self.vad_type.as_ref()
            )));
        }

        let (frame_size, aggressiveness, proportion) = self.prep_webrtc();
        Ok(WebRtcBuilder::new()
            .with_sample_rate(WebRtcSampleRate::R16kHz)
            .with_frame_length_millis(frame_size)
            .with_filter_aggressiveness(aggressiveness)
            .with_voiced_proportion_threshold(proportion)
            .build_webrtc()?)
    }

    // pub(crate) fn build_earshot(&self) -> Result<Earshot, RibbleError> {
    //     if !matches!(self.vad_type, VadType::Earshot | VadType::Auto) {
    //         return Err(RibbleError::Core(format!(
    //             "Vad type mismatch, cannot build Earshot using: {}",
    //             self.vad_type.as_ref()
    //         )));
    //     }
    //
    //     let (frame_size, aggressiveness, probability) = self.prep_webrtc();
    //     Ok(WebRtcBuilder::new()
    //         .with_sample_rate(WebRtcSampleRate::R16kHz)
    //         .with_frame_length_millis(frame_size)
    //         .with_filter_aggressiveness(aggressiveness)
    //         .with_voiced_proportion_threshold(probability)
    //         .build_earshot()?)
    // }
}

impl Default for VadConfigs {
    fn default() -> Self {
        Self::new()
    }
}


// NOTE: Earshot has some integer overflow problems.
// Until those errors get fixed, do not expose it as a VAD impl.
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    EnumIter,
    IntoStaticStr,
    AsRefStr,
    Display,
)]
pub(crate) enum VadType {
    Auto,
    Silero,
    WebRtc,
    // Earshot,
}

impl VadType {
    pub(crate) fn tooltip(&self) -> &'static str {
        match self {
            VadType::Auto => "Use the default algorithm.",
            VadType::Silero => {
                "High accuracy, high overhead.\nLeast susceptible to noise but struggles with quiet audio."
            }
            VadType::WebRtc => "Great accuracy, low overhead.\n Recommended for all purposes.",
            // VadType::Earshot => "Lower accuracy, lowest overhead.\n Good for all purposes.",
        }
    }
}

#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    EnumIter,
    IntoStaticStr,
    AsRefStr,
    Display,
)]
pub(crate) enum VadFrameSize {
    Auto,
    Small,
    Medium,
    Large,
}

#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    EnumIter,
    IntoStaticStr,
    AsRefStr,
    Display,
)]
pub(crate) enum VadStrictness {
    Auto,
    Flexible,
    Medium,
    Strict,
}

// NOTE: this enum doesn't really provide the benefit it purports to be, but it does save on mental
// load
//
// This may eventually be preferred, but the going implementation performs the dispatch earlier for
// speed and to reduce the memory footprint.
// This enum, even with boxing, is a bit too large for my liking.
pub(crate) enum RibbleVAD {
    Silero(Silero),
    WebRtc(WebRtc),
    Earshot(Box<Earshot>),
}

impl<T: PcmS16Convertible + RecorderSample> VAD<T> for RibbleVAD {
    fn voice_detected(&mut self, samples: &[T]) -> bool {
        match self {
            Self::Silero(vad) => vad.voice_detected(samples),
            Self::WebRtc(vad) => vad.voice_detected(samples),
            Self::Earshot(vad) => vad.as_mut().voice_detected(samples),
        }
    }
    fn extract_voiced_frames(&mut self, samples: &[T]) -> Box<[T]> {
        match self {
            Self::Silero(vad) => vad.extract_voiced_frames(samples),
            Self::WebRtc(vad) => vad.extract_voiced_frames(samples),
            Self::Earshot(vad) => vad.as_mut().extract_voiced_frames(samples),
        }
    }
}

impl Resettable for RibbleVAD {
    fn reset_session(&mut self) {
        match self {
            Self::Silero(vad) => vad.reset_session(),
            Self::WebRtc(vad) => vad.reset_session(),
            Self::Earshot(vad) => vad.as_mut().reset_session(),
        }
    }
}

// This ZST is just to reduce the size of the option used for "No offline VAD" branch in the
// transcriber engine.
pub(crate) struct NopVAD;
impl<T: PcmS16Convertible + RecorderSample> VAD<T> for NopVAD {
    fn voice_detected(&mut self, _samples: &[T]) -> bool {
        false
    }

    fn extract_voiced_frames(&mut self, _samples: &[T]) -> Box<[T]> {
        Box::default()
    }
}

impl Resettable for NopVAD {
    fn reset_session(&mut self) {}
}
