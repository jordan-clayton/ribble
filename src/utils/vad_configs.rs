use crate::utils::errors::RibbleError;
use ribble_whisper::audio::pcm::PcmS16Convertible;
use ribble_whisper::transcriber::vad::{
    Earshot, Resettable, Silero, SileroBuilder, VAD, WebRtc, WebRtcBuilder,
    WebRtcFilterAggressiveness, WebRtcFrameLengthMillis, WebRtcSampleRate,
};

// TODO: this should probably be re-thought; the constants dump is a little less than ideal.
use ribble_whisper::utils::constants::{
    OFFLINE_VOICE_PROBABILITY_THRESHOLD, SILERO_CHUNK_SIZE, SILERO_VOICE_PROBABILITY_THRESHOLD,
    WEBRTC_VOICE_PROBABILITY_THRESHOLD, WHISPER_SAMPLE_RATE,
};
use strum::{AsRefStr, Display, EnumIter, IntoStaticStr};

// NOTE: this should probably be kept/modified separately for Offline/Real-time configurations.
// Use a toggle in the UI to swap between Real-time VAD and Offline-Vad
// Offline can turn this off.
#[derive(Copy, Clone)]
pub(crate) struct VadConfigs {
    vad_type: VadType,
    frame_size: VadFrameSize,
    strictness: VadStrictness,
    use_vad_offline: bool,
}

impl VadConfigs {
    const STRICTEST_PROBABILITY: f32 = 0.85f32;
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
            VadFrameSize::Small | VadFrameSize::Auto => WebRtcFrameLengthMillis::MS10,
            VadFrameSize::Medium => WebRtcFrameLengthMillis::MS20,
            VadFrameSize::Large => WebRtcFrameLengthMillis::MS30,
        };

        let (aggressiveness, probability) = match self.strictness() {
            VadStrictness::Flexible => (
                WebRtcFilterAggressiveness::LowBitrate,
                WEBRTC_VOICE_PROBABILITY_THRESHOLD,
            ),
            VadStrictness::Auto | VadStrictness::Medium => (
                WebRtcFilterAggressiveness::Aggressive,
                WEBRTC_VOICE_PROBABILITY_THRESHOLD,
            ),
            // NOTE: Since the Aggressiveness does a lot of the work here with determining
            // "speech", the probability here operates as the "proportion of voiced-frames"
            // rather than an actual probability measurement like Silero.
            //
            // If it is the case that the number of voiced frames @ VeryAggressive is greater than ~60%, then it's a
            // voiced sample.
            // -- If this proves to be a bit too high (it shouldn't be), reduce this down to 50% to
            // match Silero's min frame proportion.
            VadStrictness::Strict => (
                WebRtcFilterAggressiveness::VeryAggressive,
                WEBRTC_VOICE_PROBABILITY_THRESHOLD,
            ),
        };

        (frame_size, aggressiveness, probability)
    }

    pub(crate) fn build_vad(&self) -> Result<RibbleVAD, RibbleError> {
        match self.vad_type() {
            VadType::Auto | VadType::Silero => {
                let frame_size = match self.frame_size() {
                    VadFrameSize::Small | VadFrameSize::Auto => SILERO_CHUNK_SIZE,
                    VadFrameSize::Medium => 768usize,
                    VadFrameSize::Large => 1024usize,
                };

                let probability = match self.strictness() {
                    VadStrictness::Auto | VadStrictness::Flexible => {
                        SILERO_VOICE_PROBABILITY_THRESHOLD
                    }
                    VadStrictness::Medium => OFFLINE_VOICE_PROBABILITY_THRESHOLD,
                    // TODO: determine whether to abstract the numbers into constants
                    VadStrictness::Strict => Self::STRICTEST_PROBABILITY,
                };

                let vad = SileroBuilder::new()
                    .with_sample_rate(WHISPER_SAMPLE_RATE as i64)
                    .with_chunk_size(frame_size)
                    .with_detection_probability_threshold(probability)
                    .build()
                    .into()?;
                Ok(RibbleVAD::Silero(vad))
            }
            VadType::WebRtc => {
                let (frame_size, aggressiveness, probability) = self.prep_webrtc();
                let vad = WebRtcBuilder::new()
                    .with_sample_rate(WebRtcSampleRate::R16kHz)
                    .with_frame_length_millis(frame_size)
                    .with_filter_aggressiveness(aggressiveness)
                    .with_detection_probability_threshold(probability)
                    .build_webrtc()
                    .into()?;
                Ok(RibbleVAD::WebRtc(vad))
            }
            VadType::Earshot => {
                let (frame_size, aggressiveness, probability) = self.prep_webrtc();
                let vad = WebRtcBuilder::new()
                    .with_sample_rate(WebRtcSampleRate::R16kHz)
                    .with_frame_length_millis(frame_size)
                    .with_filter_aggressiveness(aggressiveness)
                    .with_detection_probability_threshold(probability)
                    .build_earshot()
                    .into()?;

                Ok(RibbleVAD::Earshot(vad))
            }
        }
    }
}

impl Default for VadConfigs {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, EnumIter, IntoStaticStr, AsRefStr, Display)]
pub(crate) enum VadType {
    Auto,
    Silero,
    WebRtc,
    Earshot,
}

#[derive(Clone, Copy, EnumIter, IntoStaticStr, AsRefStr, Display)]
pub(crate) enum VadFrameSize {
    Auto,
    Small,
    Medium,
    Large,
}

#[derive(Clone, Copy, EnumIter, IntoStaticStr, AsRefStr, Display)]
pub(crate) enum VadStrictness {
    Auto,
    Flexible,
    Medium,
    Strict,
}

pub(crate) enum RibbleVAD {
    Silero(Silero),
    WebRtc(WebRtc),
    Earshot(Earshot),
}

impl<T: PcmS16Convertible> VAD<T> for RibbleVAD {
    fn voice_detected(&mut self, samples: &[T]) -> bool {
        match self {
            Self::Silero(vad) => vad.voice_detected(samples),
            Self::WebRtc(vad) => vad.voice_detected(samples),
            Self::Earshot(vad) => vad.voice_detected(samples),
        }
    }
    fn extract_voiced_frames(&mut self, samples: &[T]) -> Box<[T]> {
        match self {
            Self::Silero(vad) => vad.extract_voiced_frames(samples),
            Self::WebRtc(vad) => vad.extract_voiced_frames(samples),
            Self::Earshot(vad) => vad.extract_voiced_frames(samples),
        }
    }
}

impl Resettable for RibbleVAD {
    fn reset_session(&mut self) {
        match self {
            Self::Silero(vad) => vad.reset_session(),
            Self::WebRtc(vad) => vad.reset_session(),
            Self::Earshot(vad) => vad.reset_session(),
        }
    }
}
