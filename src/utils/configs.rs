#[derive(Copy, Clone, PartialEq)]
pub enum AudioWorkerType {
    REALTIME,
    STATIC,
    RECORDING,
}

#[derive(Copy, Clone, PartialEq)]
pub enum WhisperConfigType {
    REALTIME,
    STATIC,
}

#[derive(Copy, Clone, PartialEq)]
pub enum RecordingFormat {
    I16,
    I32,
    F32,
}

#[derive(Copy, Clone)]
pub struct RecorderConfigs {}