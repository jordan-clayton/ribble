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
