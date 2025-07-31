use crate::utils::errors::RibbleError;
use ribble_whisper::audio::audio_backend::CaptureSpec;
use ribble_whisper::audio::microphone::MicCapture;
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoStaticStr};

// NOTE: the From<_> implementations may not be the most logically sound.
// However, to limit the granularity of settings and to reduce the amount of
// typing (& excessive Traits that achieve the same thing), these members
// implement From to map user-facing selections to internal settings.

#[derive(
    Default,
    Copy,
    Clone,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    EnumString,
    Display,
    EnumIter,
    AsRefStr,
    IntoStaticStr,
)]
pub enum RibbleRecordingExportFormat {
    #[default]
    F32,
    I16,
}

impl RibbleRecordingExportFormat {
    pub fn tooltip(&self) -> &str {
        match self {
            RibbleRecordingExportFormat::F32 => {
                "32-bit floating point format. Highest dynamic range but large file size."
            }

            RibbleRecordingExportFormat::I16 => "16-bit signed integer format. Audio CD quality.",
        }
    }

    pub fn bits_per_sample(&self) -> u16 {
        match self {
            RibbleRecordingExportFormat::F32 => 32,
            RibbleRecordingExportFormat::I16 => 16,
        }
    }
}

impl From<hound::SampleFormat> for RibbleRecordingExportFormat {
    fn from(data: hound::SampleFormat) -> Self {
        match data {
            hound::SampleFormat::Float => RibbleRecordingExportFormat::F32,
            hound::SampleFormat::Int => RibbleRecordingExportFormat::I16,
        }
    }
}

impl From<RibbleRecordingExportFormat> for hound::SampleFormat {
    fn from(data: RibbleRecordingExportFormat) -> Self {
        match data {
            RibbleRecordingExportFormat::F32 => hound::SampleFormat::Float,
            RibbleRecordingExportFormat::I16 => hound::SampleFormat::Int,
        }
    }
}

#[derive(
    Default,
    Copy,
    Clone,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    EnumString,
    Display,
    EnumIter,
    AsRefStr,
    IntoStaticStr,
)]
pub enum RibbleChannels {
    #[default]
    Auto,
    Mono,
    Stereo,
}

impl RibbleChannels {
    pub fn into_num_channels(self) -> Option<u8> {
        match self {
            RibbleChannels::Auto => None,
            RibbleChannels::Mono => Some(1),
            RibbleChannels::Stereo => Some(2),
        }
    }
}

impl From<Option<u8>> for RibbleChannels {
    fn from(value: Option<u8>) -> Self {
        match value {
            Some(1) => RibbleChannels::Mono,
            Some(2) => RibbleChannels::Stereo,
            None | _ => RibbleChannels::Auto,
        }
    }
}

impl From<RibbleChannels> for Option<u8> {
    fn from(value: RibbleChannels) -> Self {
        value.into_num_channels()
    }
}

#[derive(
    Default,
    Copy,
    Clone,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    EnumString,
    Display,
    EnumIter,
    AsRefStr,
    IntoStaticStr,
)]
pub enum RibbleSampleRate {
    #[default]
    Auto,
    Low,
    Medium,
    High,
    Highest,
}

impl RibbleSampleRate {
    pub fn into_sample_rate(self) -> Option<usize> {
        match self {
            RibbleSampleRate::Auto => None,
            RibbleSampleRate::Low => Some(8000),
            RibbleSampleRate::Medium => Some(16000),
            RibbleSampleRate::High => Some(22050),
            RibbleSampleRate::Highest => Some(44100),
        }
    }
}

impl From<Option<usize>> for RibbleSampleRate {
    fn from(value: Option<usize>) -> Self {
        match value {
            Some(8000) => RibbleSampleRate::Low,
            Some(16000) => RibbleSampleRate::Medium,
            Some(22050) => RibbleSampleRate::High,
            Some(44100) => RibbleSampleRate::Highest,
            None | _ => RibbleSampleRate::Auto,
        }
    }
}

impl From<RibbleSampleRate> for Option<usize> {
    fn from(value: RibbleSampleRate) -> Self {
        value.into_sample_rate()
    }
}

#[derive(
    Default,
    Copy,
    Clone,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    EnumString,
    Display,
    EnumIter,
    AsRefStr,
    IntoStaticStr,
)]
pub enum RibblePeriod {
    #[default]
    Auto,
    Small,
    Medium,
    Large,
    Huge,
}

impl RibblePeriod {
    pub fn into_period(self) -> Option<usize> {
        match self {
            RibblePeriod::Auto => None,
            RibblePeriod::Small => Some(512),
            RibblePeriod::Medium => Some(1024),
            RibblePeriod::Large => Some(2048),
            RibblePeriod::Huge => Some(4096),
        }
    }
}

impl From<Option<usize>> for RibblePeriod {
    fn from(value: Option<usize>) -> Self {
        match value {
            Some(512) => RibblePeriod::Small,
            Some(1024) => RibblePeriod::Medium,
            Some(2048) => RibblePeriod::Large,
            Some(4096) => RibblePeriod::Huge,
            _ => RibblePeriod::Auto,
        }
    }
}

impl From<RibblePeriod> for Option<usize> {
    fn from(value: RibblePeriod) -> Self {
        value.into_period()
    }
}

#[derive(Default, Copy, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RibbleRecordingConfigs {
    sample_rate: RibbleSampleRate,
    channel_configs: RibbleChannels,
    period: RibblePeriod,
}

impl RibbleRecordingConfigs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_mic_capture<M: MicCapture>(capture: &M) -> Self {
        let sample_rate = Some(capture.sample_rate()).into();
        let channel_configs = Some(capture.channels()).into();
        let period = Some(capture.buffer_size()).into();

        Self {
            sample_rate,
            channel_configs,
            period,
        }
    }

    pub fn with_sample_rate(mut self, sample_rate: RibbleSampleRate) -> Self {
        self.sample_rate = sample_rate;
        self
    }
    pub fn with_num_channels(mut self, channel_configs: RibbleChannels) -> Self {
        self.channel_configs = channel_configs;
        self
    }
    pub fn with_period(mut self, period: RibblePeriod) -> Self {
        self.period = period;
        self
    }

    pub fn sample_rate(&self) -> RibbleSampleRate {
        self.sample_rate
    }
    pub fn num_channels(&self) -> RibbleChannels {
        self.channel_configs
    }
    pub fn period(&self) -> RibblePeriod {
        self.period
    }

    pub fn into_wav_spec(
        self,
        format: RibbleRecordingExportFormat,
    ) -> Result<hound::WavSpec, RibbleError> {
        let bits_per_sample = format.bits_per_sample();
        let sample_format = format.into();

        let channels: Option<u8> = self.channel_configs.into();
        let channels = channels.ok_or(RibbleError::Core(
            "Invalid channel options passed to file writer.".to_string(),
        ))? as u16;

        let sample_rate: Option<usize> = self.sample_rate.into();
        let sample_rate = sample_rate.ok_or(RibbleError::Core(
            "Invalid sample rate options passed to file writer.".to_string(),
        ))? as u32;

        Ok(hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample,
            sample_format,
        })
    }
}

impl From<CaptureSpec> for RibbleRecordingConfigs {
    fn from(value: CaptureSpec) -> Self {
        let sample_rate = value.sample_rate().into();
        let num_channels = value.channels().into();
        let period = value.period().into();
        Self::new()
            .with_sample_rate(sample_rate)
            .with_num_channels(num_channels)
            .with_period(period)
    }
}

impl From<RibbleRecordingConfigs> for CaptureSpec {
    fn from(value: RibbleRecordingConfigs) -> Self {
        Self::new()
            .with_sample_rate(value.sample_rate.into())
            .with_num_channels(value.channel_configs.into())
            .with_period(value.period.into())
    }
}
