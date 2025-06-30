use ribble_whisper::audio::audio_backend::CaptureSpec;
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoStaticStr};

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
pub(crate) enum RibbleRecordingFormat {
    #[default]
    F32,
    I16,
}

impl RibbleRecordingFormat {
    pub fn tooltip(&self) -> &str {
        match self {
            RibbleRecordingFormat::F32 => {
                "32-bit floating point format. Highest dynamic range but large file size."
            }

            RibbleRecordingFormat::I16 => "16-bit signed integer format. Audio CD quality.",
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
pub(crate) enum RibbleChannels {
    #[default]
    Auto,
    Mono,
    Stereo,
}

impl RibbleChannels {
    pub(crate) fn into_num_channels(self) -> Option<u8> {
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
pub(crate) enum RibbleSampleRate {
    #[default]
    Auto,
    Low,
    Medium,
    High,
    Highest,
}

impl RibbleSampleRate {
    pub(crate) fn into_sample_rate(self) -> Option<usize> {
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
    pub(crate) fn into_period(self) -> Option<usize> {
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
        }
    }
}

impl From<RibblePeriod> for Option<usize> {
    fn from(value: RibblePeriod) -> Self {
        value.into_period()
    }
}

#[derive(Default, Copy, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct RibbleRecordingConfigs {
    sample_rate: RibbleSampleRate,
    channel_configs: RibbleChannels,
    period: RibblePeriod,
}

impl RibbleRecordingConfigs {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_sample_rate(mut self, sample_rate: RibbleSampleRate) -> Self {
        self.sample_rate = sample_rate;
        self
    }
    pub(crate) fn with_num_channels(mut self, channel_configs: RibbleChannels) -> Self {
        self.channel_configs = channel_configs;
        self
    }
    pub(crate) fn with_period(mut self, period: RibblePeriod) -> Self {
        self.period = period;
        self
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
