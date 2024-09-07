use atomic_enum::atomic_enum;

#[atomic_enum]
pub enum AudioWorkerState {
    Idle,
    Loading,
    Running,
    Error,
}
