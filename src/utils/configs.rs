use strum::{Display, EnumIter};

use crate::utils::constants;

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Display, EnumIter)]
pub enum LoadingState {
    NotLoaded,
    Loading,
    Loaded,
}

// TODO: Remove this if unused - Refactor impl to use a bg joiner thread.
#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Display, EnumIter)]
pub enum WorkerType {
    Downloading,
    Realtime,
    Static,
    Recording,
}

impl WorkerType {
    pub fn to_key(&self) -> &str {
        match self {
            WorkerType::Downloading => "downloading",
            WorkerType::Realtime => "realtime",
            WorkerType::Static => "static",
            WorkerType::Recording => "recording",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum AudioConfigs {
    Realtime(whisper_realtime::configs::Configs),
    Static(whisper_realtime::configs::Configs),
    Recording(RecorderConfigs),
}

impl AudioConfigs {
    pub fn is_realtime(&self) -> bool {
        matches!(*self, AudioConfigs::Realtime(_))
    }

    pub fn is_static(&self) -> bool {
        matches!(*self, AudioConfigs::Static(_))
    }

    pub fn is_recording(&self) -> bool {
        matches!(*self, AudioConfigs::Recording(_))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AudioConfigType {
    Realtime,
    Static,
    Recording,
}

#[derive(
    Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Display, EnumIter,
)]
pub enum RecordingFormat {
    I16,
    I32,
    F32,
}

impl Default for RecordingFormat {
    fn default() -> Self {
        Self::I16
    }
}

impl RecordingFormat {
    pub fn tooltip(&self) -> &str {
        match self {
            RecordingFormat::I16 => "16-bit signed integer format. Audio CD quality.",
            RecordingFormat::I32 => {
                "32-bit signed integer format. Wider dynamic range, but slower to process."
            }
            RecordingFormat::F32 => {
                "32-bit floating point format. Highest dynamic range, but large file size."
            }
        }
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Display, EnumIter,
)]
pub enum Channel {
    Default,
    Mono,
    Stereo,
}

impl Channel {
    fn num_channels(&self) -> Option<u8> {
        match self {
            Channel::Default => None,
            Channel::Mono => Some(1),
            Channel::Stereo => Some(2),
        }
    }
}

impl Default for Channel {
    fn default() -> Self {
        Self::Default
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Display, EnumIter,
)]
pub enum SampleRate {
    Default,
    Low,
    Medium,
    High,
    Highest,
}

impl SampleRate {
    pub fn sample_rate(&self) -> Option<i32> {
        match self {
            SampleRate::Default => None,
            SampleRate::Low => Some(8000),
            SampleRate::Medium => Some(16000),
            SampleRate::High => Some(22050),
            SampleRate::Highest => Some(44100),
        }
    }
}

impl Default for SampleRate {
    fn default() -> Self {
        Self::Default
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Display, EnumIter,
)]
pub enum BufferSize {
    Default,
    Small,
    Medium,
    Large,
    Huge,
}

impl BufferSize {
    pub fn size(&self) -> Option<u16> {
        match self {
            BufferSize::Default => None,
            BufferSize::Small => Some(512),
            BufferSize::Medium => Some(1024),
            BufferSize::Large => Some(2048),
            BufferSize::Huge => Some(4096),
        }
    }
}

impl Default for BufferSize {
    fn default() -> Self {
        Self::Default
    }
}

#[derive(Copy, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RecorderConfigs {
    pub sample_rate: SampleRate,
    pub buffer_size: BufferSize,
    pub channel: Channel,
    pub format: RecordingFormat,
    pub filter: bool,
    pub f_lower: f32,
    pub f_higher: f32,
}

impl RecorderConfigs {
    pub fn extract_sample_rate(&self) -> Option<i32> {
        self.sample_rate.sample_rate()
    }
    pub fn extract_num_channels(&self) -> Option<u8> {
        self.channel.num_channels()
    }

    pub fn extract_buffer_size(&self) -> Option<u16> {
        self.buffer_size.size()
    }
}

impl Default for RecorderConfigs {
    fn default() -> Self {
        let sample_rate = SampleRate::default();
        let buffer_size = BufferSize::default();
        let channel = Channel::default();
        let format = RecordingFormat::default();
        let filter = false;
        let f_lower = constants::DEFAULT_F_LOWER;
        let f_higher = constants::DEFAULT_F_HIGHER;
        Self {
            sample_rate,
            buffer_size,
            channel,
            format,
            filter,
            f_lower,
            f_higher,
        }
    }
}
