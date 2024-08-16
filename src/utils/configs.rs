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

#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RecordingFormat {
    I16,
    I32,
    F32,
}

#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Channel {
    DEFAULT,
    MONO,
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

#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SampleRate {
    DEFAULT,
    LOW,
    MEDIUM,
    HIGH,
    HIGHEST,
}

impl SampleRate {
    fn sample_rate(&self) -> Option<i32> {
        match self {
            SampleRate::DEFAULT => None,
            SampleRate::LOW => Some(8000),
            SampleRate::MEDIUM => Some(16000),
            SampleRate::HIGH => Some(22050),
            SampleRate::HIGHEST => Some(44100),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BufferSize {
    DEFAULT,
    SMALL,
    MEDIUM,
    LARGE,
    HUGE,
}

impl BufferSize {
    fn size(&self) -> Option<u16> {
        match self {
            BufferSize::DEFAULT => None,
            BufferSize::SMALL => Some(256),
            BufferSize::MEDIUM => Some(512),
            BufferSize::LARGE => Some(1024),
            BufferSize::HUGE => Some(2048),
        }
    }
}

#[derive(Copy, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RecorderConfigs {
    sample_rate: SampleRate,
    buffer_size: BufferSize,
    channel: Channel,
    format: RecordingFormat,
}

impl RecorderConfigs {
    pub fn sample_rate(&self) -> Option<i32> {
        self.sample_rate.sample_rate()
    }
    pub fn num_channels(&self) -> Option<u8> {
        self.channel.num_channels()
    }

    pub fn buffer_size(&self) -> Option<u16> {
        self.buffer_size.size()
    }
}

impl Default for RecorderConfigs {
    fn default() -> Self {
        todo!()
    }
}
