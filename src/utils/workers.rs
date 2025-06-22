use atomic_enum::atomic_enum;
// TODO: Remove this -- it's unlikely to be genuinely useful.
#[atomic_enum]
pub enum AudioWorkerState {
    Idle,
    Loading,
    Running,
    Error,
}
