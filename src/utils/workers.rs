use atomic_enum::atomic_enum;
use strum::{Display, EnumIter};

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Display, EnumIter)]
pub enum WorkerType {
    Downloading,
    Saving,
    Realtime,
    Static,
    Recording,
    IO,
}

#[atomic_enum]
pub enum AudioWorkerState {
    Idle,
    Loading,
    Running,
    Error,
}

#[atomic_enum]
pub enum AudioWorkerType {
    Realtime,
    Static,
    Recording,
}
