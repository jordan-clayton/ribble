use ribble_whisper::audio::pcm::IntoPcmS16;

pub(crate) trait IntoPcmF32 {
    fn into_pcm_f32(self) -> f32;
}

pub(crate) trait FromPcmF32 {
    fn from_pcm_f32(sample: f32) -> Self;
}

pub(crate) trait PcmF32Convertible: IntoPcmF32 + FromPcmF32 {}

impl IntoPcmF32 for f32 {
    fn into_pcm_f32(self) -> f32 {
        self
    }
}

impl FromPcmF32 for f32 {
    fn from_pcm_f32(sample: f32) -> Self {
        sample
    }
}

impl PcmF32Convertible for f32 {}

impl IntoPcmF32 for i16 {
    fn into_pcm_f32(self) -> f32 {
        (self as f32 / i16::MAX as f32).clamp(-1f32, 1f32)
    }
}

impl FromPcmF32 for i16 {
    fn from_pcm_f32(sample: f32) -> Self {
        sample.into_pcm_s16()
    }
}

impl PcmF32Convertible for i16 {}