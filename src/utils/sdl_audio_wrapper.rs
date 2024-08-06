// Sdl audio is documented to be thread safe.
pub struct SdlAudioWrapper {
    pub audio_subsystem: sdl2::AudioSubsystem,
}

unsafe impl Send for SdlAudioWrapper {}

unsafe impl Sync for SdlAudioWrapper {}