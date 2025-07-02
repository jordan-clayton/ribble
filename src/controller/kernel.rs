use crate::controller::console::ConsoleEngine;
use crate::controller::downloader::DownloadEngine;
use crate::controller::progress::ProgressEngine;
use crate::controller::recorder::RecorderEngine;
use crate::controller::transcriber::TranscriberEngine;
use crate::controller::visualizer::VisualizerEngine;
use crate::controller::worker::WorkerEngine;
use crate::utils::audio_backend_proxy::AudioBackendProxy;
use crate::utils::model_bank::RibbleModelBank;
use std::path::Path;
use std::sync::Arc;

// TODO: move this to user prefs?
// TODO: on second thought, probably should remove.
pub(crate) enum TranscriberMethod {
    Realtime,
    Offline,
}

// TODO: NOTE TO SELF, store this in the controller instead of the old spaghetti.
// The spaghetti is now portioned onto different plates, to make things easier to manage.
pub(super) struct Kernel {
    // NOTE: pass this -in- to the RecorderEngine/TranscriberEngine as arguments
    audio_backend: AudioBackendProxy,

    // The Whisper configs are also serializeable -> derive serialize on the struct and write
    // a routine in Transcriber engine or pass the configs in.
    transcriber_engine: TranscriberEngine,
    recorder_engine: RecorderEngine,
    console_engine: ConsoleEngine,
    progress_engine: ProgressEngine,
    visualizer_engine: VisualizerEngine,
    worker_engine: WorkerEngine,
    download_engine: DownloadEngine,
    // Since model bank needs to be accessed elsewhere (e.g. TranscriberEngine), this needs to be
    // in a shared pointer.
    model_bank: Arc<RibbleModelBank>,
}

impl Kernel {
    pub(super) fn new() -> Self {
        todo!("Implement the Kernel constructor.");
        // Basic routine:
        // - Initialize Message queues for bus
        // - Make a bus
        // - Try-Load the app state -> all-together in one hashmap?
        //   (Configs, Online/Offline VadConfigs, UserPreferences)
        // - Construct all the engines (pass in the bus + extra config params)
        // - Voila.
    }

    // TODO: remaining delegate methods: Downloading models, adding new models.
    // NOTE: most of these spawn worker threads in the background to do all work.
    pub(super) fn start_realtime_transcription(&self) {
        let bank = Arc::clone(&self.model_bank);

        self.transcriber_engine
            .start_realtime_transcription(&self.audio_backend, bank);
    }

    pub(super) fn start_offline_transcription(&self, audio_file: Path) {
        let bank = Arc::clone(&self.model_bank);
        self.transcriber_engine
            .start_offline_transcription(audio_file, bank);
    }

    pub(super) fn start_recording(&self) {
        self.recorder_engine.start_recording(&self.audio_backend);
    }
}
