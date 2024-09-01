use atomic_enum::atomic_enum;
use strum::{Display, EnumIter};

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Display, EnumIter)]
pub enum WorkerType {
    Downloading,
    Saving,
    Realtime,
    Static,
    Recording,
    ThreadManagement,
}

// TODO: finish this
#[atomic_enum]
pub enum WorkerState {
    Idle,
    Loading,
    Running,
}

// TODO: Remove if unnecessary
#[atomic_enum]
pub enum AudioWorkerType {
    Realtime,
    Static,
    Recording,
}

