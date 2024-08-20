use strum::{Display, EnumIter};

// TODO: Remove this if unused - Refactor impl to use a bg joiner thread.
#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WorkerType {
    DOWNLOADING,
    REALTIME,
    STATIC,
    RECORDING,
}

impl WorkerType {
    pub fn to_key(&self) -> &str {
        match self {
            WorkerType::DOWNLOADING => "downloading",
            WorkerType::REALTIME => "realtime",
            WorkerType::STATIC => "static",
            WorkerType::RECORDING => "recording",
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
    REALTIME,
    STATIC,
    RECORDING,
}

#[derive(
    Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Display, EnumIter,
)]
pub enum RecordingFormat {
    I16,
    I32,
    F32,
}

impl RecordingFormat {
    pub fn tooltip(&self) -> &str {
        match self {
            RecordingFormat::I16 => { "16-bit signed integer format. Audio CD quality." }
            RecordingFormat::I32 => { "32-bit signed integer format. Wider dynamic range, but slower to process." }
            RecordingFormat::F32 => { "32-bit floating point format. Highest dynamic range, but large file size." }
        }
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Display, EnumIter,
)]
pub enum Channel {
    #[strum(to_string = "Default")]
    DEFAULT,
    #[strum(to_string = "Mono")]
    MONO,
    #[strum(to_string = "Stereo")]
    STEREO,
}

impl Channel {
    fn num_channels(&self) -> Option<u8> {
        match self {
            Channel::DEFAULT => None,
            Channel::MONO => Some(1),
            Channel::STEREO => Some(2),
        }
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Display, EnumIter,
)]
pub enum SampleRate {
    #[strum(to_string = "Default")]
    DEFAULT,
    #[strum(to_string = "Low")]
    LOW,
    #[strum(to_string = "Medium")]
    MEDIUM,
    #[strum(to_string = "High")]
    HIGH,
    #[strum(to_string = "Highest")]
    HIGHEST,
}

impl SampleRate {
    pub fn sample_rate(&self) -> Option<i32> {
        match self {
            SampleRate::DEFAULT => None,
            SampleRate::LOW => Some(8000),
            SampleRate::MEDIUM => Some(16000),
            SampleRate::HIGH => Some(22050),
            SampleRate::HIGHEST => Some(44100),
        }
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Display, EnumIter,
)]
pub enum BufferSize {
    #[strum(to_string = "Default")]
    DEFAULT,
    #[strum(to_string = "Small")]
    SMALL,
    #[strum(to_string = "Medium")]
    MEDIUM,
    #[strum(to_string = "Large")]
    LARGE,
    #[strum(to_string = "Huge")]
    HUGE,
}

impl BufferSize {
    pub fn size(&self) -> Option<u16> {
        match self {
            BufferSize::DEFAULT => None,
            BufferSize::SMALL => Some(512),
            BufferSize::MEDIUM => Some(1024),
            BufferSize::LARGE => Some(2048),
            BufferSize::HUGE => Some(4096),
        }
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
        todo!()
    }
}
