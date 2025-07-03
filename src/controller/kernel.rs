use crate::controller::console::ConsoleEngine;
use crate::controller::downloader::DownloadEngine;
use crate::controller::progress::ProgressEngine;
use crate::controller::recorder::RecorderEngine;
use crate::controller::transcriber::{OfflineTranscriberFeedback, TranscriberEngine};
use crate::controller::visualizer::VisualizerEngine;
use crate::controller::worker::WorkerEngine;
use crate::controller::Bus;
use crate::utils::audio_backend_proxy::{AudioBackendProxy, AudioCaptureRequest};
use crate::utils::errors::RibbleError;
use crate::utils::model_bank::RibbleModelBank;
use crate::utils::preferences::UserPreferences;
use crate::utils::recorder_configs::RibbleRecordingConfigs;
use crate::utils::vad_configs::VadConfigs;
use arc_swap::ArcSwap;
use ribble_whisper::utils::{get_channel, Sender};
use ribble_whisper::whisper::configs::WhisperRealtimeConfigs;
use ron::ser::PrettyConfig;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
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
    data_directory: PathBuf,
    user_preferences: ArcSwap<UserPreferences>,
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
    const CONFIGS_FILE: &'static str = "ribble_configs.ron";
    const MODEL_BANK_DIR_SLUG: &'static str = "models";
    const UTILITY_QUEUE_SIZE: usize = 32;
    const SMALL_UTILITY_QUEUE_SIZE: usize = 16;
    const UI_UPDATE_QUEUE_SIZE: usize = 8;
    // TODO: determine whether or not this is necessary, whether it should be changed.
    // Right now, there are no hard limits on how large this can get.
    const DEFAULT_PROGRESS_SLAB_CAPACITY: usize = 8;

    // NOTE: this needs to take in the audio capture request sender from the app (main thread)
    // because of SDL invariants.
    pub(super) fn new(data_directory: &Path, audio_capture_request_sender: Sender<AudioCaptureRequest>) -> Result<Self, RibbleError> {
        // Basic routine:
        // - Set the data directory
        // - Initialize Message queues for bus
        // - Make a bus
        // - Try-Load the app state -> all-together in one hashmap?
        //   (Configs, Online/Offline VadConfigs, UserPreferences)
        // - Construct all the engines (pass in the bus + extra config params)
        // - Voila.

        let KernelState {
            transcriber_configs,
            offline_transcriber_feedback,
            vad_configs,
            recording_configs,
            user_preferences,
        } = Self::deserialize_user_data(data_directory);
        let (console_sender, console_receiver) = get_channel(Self::UTILITY_QUEUE_SIZE);
        let (progress_sender, progress_receiver) = get_channel(Self::SMALL_UTILITY_QUEUE_SIZE);
        let (work_sender, work_receiver) = get_channel(Self::UTILITY_QUEUE_SIZE);
        let (write_sender, write_receiver) = get_channel(Self::UTILITY_QUEUE_SIZE);
        let (visualizer_sender, visualizer_receiver) = get_channel(Self::UTILITY_QUEUE_SIZE);
        let (download_sender, download_receiver) = get_channel(Self::SMALL_UTILITY_QUEUE_SIZE);

        let audio_backend = AudioBackendProxy::new(audio_capture_request_sender);

        let bus = Bus::new(
            console_sender,
            progress_sender,
            work_sender,
            write_sender,
            visualizer_sender,
            download_sender,
        );

        let transcriber_engine = TranscriberEngine::new(
            transcriber_configs,
            vad_configs,
            offline_transcriber_feedback,
            &bus,
        );

        let recorder_engine = RecorderEngine::new(recording_configs, &bus);
        let console_engine = ConsoleEngine::new(
            console_receiver,
            user_preferences.console_message_size(),
            &bus,
        );
        let progress_engine = ProgressEngine::new(Self::DEFAULT_PROGRESS_SLAB_CAPACITY);
        let visualizer_engine = VisualizerEngine::new(visualizer_receiver);
        let worker_engine = WorkerEngine::new(work_receiver, &bus);
        let download_engine = DownloadEngine::new(download_receiver, &bus);

        let model_directory = data_directory.to_path_buf().join(Self::MODEL_BANK_DIR_SLUG);
        let model_bank = Arc::new(RibbleModelBank::new(model_directory.as_path())?);

        Ok(Self {
            data_directory: data_directory.to_path_buf(),
            user_preferences: ArcSwap::from(Arc::new(user_preferences)),
            audio_backend,
            transcriber_engine,
            recorder_engine,
            console_engine,
            progress_engine,
            visualizer_engine,
            worker_engine,
            download_engine,
            model_bank,
        })
    }

    // TODO: remaining delegate methods: Downloading models, adding new models.
    // NOTE: most of these spawn worker threads in the background to do all work.
    pub(super) fn start_realtime_transcription(&self) {
        let bank = Arc::clone(&self.model_bank);

        self.transcriber_engine
            .start_realtime_transcription(&self.audio_backend, bank);
    }

    pub(super) fn start_offline_transcription(&self, audio_file: &Path) {
        let bank = Arc::clone(&self.model_bank);
        self.transcriber_engine
            .start_offline_transcription(audio_file, bank);
    }

    pub(super) fn start_recording(&self) {
        self.recorder_engine.start_recording(&self.audio_backend);
    }

    // TODO: return a result or log.
    fn serialize_user_data(&self) {
        let transcriber_configs = *self.transcriber_engine.read_transcription_configs();
        let offline_transcriber_feedback =
            self.transcriber_engine.read_offline_transcriber_feedback();
        let vad_configs = *self.transcriber_engine.read_vad_configs();
        let recording_configs = *self.recorder_engine.read_recorder_configs();
        let user_preferences = *self.user_preferences.load_full();

        let state = KernelState {
            transcriber_configs,
            offline_transcriber_feedback,
            vad_configs,
            recording_configs,
            user_preferences,
        };

        let configs_file = File::open(self.data_directory.as_path());
        if let Ok(file) = configs_file {
            let writer = BufWriter::new(file);
            if ron::Options::default().to_io_writer_pretty(writer, &state, PrettyConfig::default()).is_err() {
                todo!("LOGGING");
            }
        } else {
            todo!("LOGGING");
        }
    }

    fn deserialize_user_data(data_directory: &Path) -> KernelState {
        let canonicalized = data_directory.to_path_buf().join(Self::CONFIGS_FILE);
        match File::open(&canonicalized) {
            Ok(file) => {
                let reader = BufReader::new(file);
                match ron::de::from_reader(reader) {
                    Ok(state) => state,
                    Err(_) => {
                        todo!("LOGGING");
                        KernelState::default()
                    }
                }
            }
            Err(_) => {
                todo!("LOGGING");
                KernelState::default()
            }
        }
    }
}

impl Drop for Kernel {
    fn drop(&mut self) {
        self.serialize_user_data();
    }
}

// Basic serializeable/deserializable app state.
// At this time, there isn't much to keep track of, but this may
// start to grow as features get added.
//
// As of now, everything does implement default, so if the resource is missing/gets lost, there's a
// fallback.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct KernelState {
    #[serde(default)]
    transcriber_configs: WhisperRealtimeConfigs,
    #[serde(default)]
    offline_transcriber_feedback: OfflineTranscriberFeedback,
    #[serde(default)]
    vad_configs: VadConfigs,
    #[serde(default)]
    recording_configs: RibbleRecordingConfigs,
    #[serde(default)]
    user_preferences: UserPreferences,
}
