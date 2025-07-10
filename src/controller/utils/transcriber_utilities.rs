use std::sync::Arc;

use crossbeam::channel::Sender;
use sdl2::AudioSubsystem;
use sdl2::audio::{AudioDevice, AudioSpecDesired};
use whisper_realtime::{
    audio_ring_buffer::AudioRingBuffer, microphone, model::Model, recorder::Recorder, whisper_rs,
};

// TODO: just get rid of this file
use crate::utils::constants;

pub fn init_audio_ring_buffer<
    T: Default + Clone + Copy + sdl2::audio::AudioFormatNum + Sync + Send + 'static,
>(
    len_ms: Option<usize>,
) -> Arc<AudioRingBuffer<T>> {
    let ms = len_ms.unwrap_or(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);
    let audio: AudioRingBuffer<T> = AudioRingBuffer::new(ms);
    Arc::new(audio)
}

pub fn init_microphone<
    T: Default + Clone + Copy + sdl2::audio::AudioFormatNum + Sync + Send + 'static,
>(
    audio_subsystem: &AudioSubsystem,
    desired_audio_spec: &AudioSpecDesired,
    audio_sender: Sender<Vec<T>>,
) -> Arc<AudioDevice<Recorder<T>>> {
    let mic_stream =
        microphone::build_audio_stream(audio_subsystem, desired_audio_spec, audio_sender);
    Arc::new(mic_stream)
}

pub fn init_realtime_microphone(
    audio_subsystem: &AudioSubsystem,
    audio_sender: Sender<Vec<f32>>,
) -> Arc<AudioDevice<Recorder<f32>>> {
    let desired_audio_spec = microphone::get_desired_audio_spec(
        Some(whisper_realtime::constants::WHISPER_SAMPLE_RATE as i32),
        Some(1),
        Some(1024),
    );

    init_microphone(audio_subsystem, &desired_audio_spec, audio_sender)
}

pub fn init_model(configs: Arc<whisper_realtime::configs::Configs>) -> Arc<Model> {
    let model_type = configs.model;
    let data_dir = eframe::storage_dir(constants::APP_ID).expect("Failed to get Data dir");
    let model = Model::new_with_type_and_dir(model_type, data_dir);
    assert!(model.is_downloaded(), "Model not downloaded");
    Arc::new(model)
}

pub fn init_whisper_ctx(model: Arc<Model>, use_gpu: bool) -> whisper_rs::WhisperContext {
    let mut whisper_ctx_params = whisper_rs::WhisperContextParameters::default();
    whisper_ctx_params.use_gpu = use_gpu;
    let model_path = model.file_path();
    let model_path = model_path.as_path();
    whisper_rs::WhisperContext::new_with_params(
        model_path.to_str().expect("Failed to stringify path"),
        whisper_ctx_params,
    )
    .expect("Failed to load model")
}
