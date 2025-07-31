use crate::controller::{Bus, ConsoleMessage, DownloadRequest, ModelFile, RibbleMessage, WorkRequest, SMALL_UTILITY_QUEUE_SIZE};
use crate::utils::errors::RibbleError;
use indexmap::IndexMap;
use parking_lot::RwLock;
use ribble_whisper::utils::errors::RibbleWhisperError;
use ribble_whisper::utils::{get_channel, Receiver, Sender};
use ribble_whisper::whisper::model::{ModelId, ModelLocation, ModelRetriever};
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use twox_hash::XxHash3_64;

use notify_debouncer_full::notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache};

const MODEL_ID_SEED: u64 = 0;
const MODEL_FILE_EXTENSION: &str = "bin";
// NOTE: THIS IS IN MILLISECONDS
// NOTE TWICE: if this is burning cycles, increase the timeout or add an explicit tick-rate that's
// close to the actual timeout.
const MAX_DEBOUNCE_TIME: u64 = 2000;

const PACKED_MODELS: [&[u8]; 2] = [
    include_bytes!("../models/ggml-base-q5_1.bin"),
    include_bytes!("../models/ggml-tiny-q5_1.bin")
];

struct RibbleModelBankState {
    model_directory: PathBuf,
    // NOTE: this isn't using ribble_whisper's model/ConcurrentModelBank abstraction.
    // It's way, way easier and cheaper to use Arc<str>
    model_map: RwLock<IndexMap<ModelId, ModelFile>>,
    model_directory_watcher: Receiver<DebounceEventResult>,
}

impl RibbleModelBankState {
    const DEFAULT_MODEL_MAP_SIZE: usize = 8;

    pub fn new(
        model_directory: &Path,
        watcher: Receiver<DebounceEventResult>,
    ) -> Result<Self, RibbleError> {
        let model_map = RwLock::new(IndexMap::with_capacity(Self::DEFAULT_MODEL_MAP_SIZE));
        let model_directory = model_directory.to_path_buf();

        // Test to make sure model_directory exists and is.
        if !fs::metadata(&model_directory)?.is_dir() {
            Err(RibbleError::IOError(std::io::Error::new(
                ErrorKind::NotADirectory,
                format!("Model path: {model_directory:?} is not a directory"),
            )))
        } else {
            Self {
                model_directory,
                model_map,
                model_directory_watcher: watcher,
            }
                .init()
        }
    }

    fn init(self) -> Result<Self, RibbleError> {
        self.fill_model_bank()?;
        Ok(self)
    }

    fn model_directory(&self) -> &Path {
        self.model_directory.as_path()
    }

    // This just checks to see if there was any major modification to the models directory.
    // On a change, it refreshes the model bank.
    fn handle_debounced_events(&self, events: &[DebouncedEvent]) {
        let changed = events.iter().any(|deb_event| match deb_event.event.kind {
            // NOTE: If it becomes imperative to limit Create() and Remove() to files and only
            // files, (right now, it's any: files/folders/any/other), then bring in notify_types.
            // For whatever reason, the types aren't publicly exposed in notify_debouncer_full.
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => true,
            // NOTE: if missing events due to OS weirdness, accept EventKind::Any (is the
            // catch-all event)
            _ => false,
        });

        if !changed {
            return;
        }

        if let Err(e) = self.refresh_model_bank() {
            log::warn!(
                "Failed to refresh model bank.\nError: {e}\nError source: {:#?}",
                e.source()
            );
        }
    }

    fn fill_model_bank(&self) -> Result<(), std::io::Error> {
        let data_directory = &self.model_directory;

        // Read all the valid entries, filter out and extract the file_name and file size
        let mut entries = fs::read_dir(data_directory)?
            // Get entries in the directory
            .filter_map(Result::ok)
            // Filter by file & file extension is .bin
            .filter(|entry| {
                entry.path().is_file() && entry.path().extension() == Some(OsStr::new("bin"))
            })
            // Filter for only valid file names
            .filter(|entry| entry.path().file_name().is_some())
            // Filter out for invalid metadata
            .filter(|entry| entry.metadata().is_ok())
            // Only get valid Unicode file paths
            .filter(|entry| entry.path().file_name().unwrap().to_str().is_some())
            // Map into metadata.
            .map(|entry| {
                (
                    ModelFile::File(Arc::from(entry.path().file_name().unwrap().to_str().unwrap())),
                    entry.metadata().unwrap().len(),
                )
            })
            .collect::<Vec<(ModelFile, u64)>>();

        // Push entries for the two asset-packed models.
        for (idx, model) in PACKED_MODELS.iter().enumerate() {
            let packed = ModelFile::Packed(idx);
            let size = model.len() as u64;
            entries.push((packed, size))
        };

        // Sort by file size.
        entries.sort_by(|(_, size1), (_, size2)| size1.cmp(size2));

        // Get a write lock to fill the map.
        let mut map_lock = self.model_map.write();

        // Fill the model bank.
        for (model_file, _) in entries {
            let model_key = match &model_file {
                ModelFile::Packed(idx) => {
                    XxHash3_64::oneshot_with_seed(MODEL_ID_SEED, ModelFile::PACKED_NAMES[*idx].as_bytes())
                }
                ModelFile::File(file_name) => {
                    XxHash3_64::oneshot_with_seed(MODEL_ID_SEED, file_name.as_ref().as_bytes())
                }
            };
            map_lock.insert(model_key, model_file);
        }

        Ok(())
    }

    fn contains_model(&self, model_id: ModelId) -> bool {
        self.model_map.read().contains_key(&model_id)
    }

    fn retrieve_model(&self, model_id: ModelId) -> Option<ModelLocation> {
        self.model_map.read().get(&model_id).map(|model| {
            match model {
                ModelFile::Packed(idx) => {
                    ModelLocation::StaticBuffer(PACKED_MODELS[*idx])
                }
                ModelFile::File(name) => {
                    ModelLocation::DynamicFilePath(self.model_directory().join(name.as_ref()))
                }
            }
        })
    }

    // NOTE: this code can probably stay, though it should be impossible for this to return false based on
    // the directory crawler.
    fn model_exists_in_storage(&self, model_id: ModelId) -> Result<bool, RibbleWhisperError> {
        if let Some(model) = self.model_map.read().get(&model_id) {
            match model {
                ModelFile::Packed(_) => Ok(true),
                ModelFile::File(path) => {
                    let full_path = self.model_directory.join(path.as_ref());
                    let metadata = fs::metadata(&full_path);
                    match metadata {
                        Ok(m) => Ok(m.is_file()),
                        Err(e) => match e.kind() {
                            ErrorKind::NotFound => Ok(false),
                            _ => Err(RibbleWhisperError::IOError(e)),
                        },
                    }
                }
            }
        } else {
            Ok(false)
        }
    }

    fn clear(&self) {
        self.model_map.write().clear();
    }

    fn refresh_model_bank(&self) -> Result<(), RibbleWhisperError> {
        self.clear();
        Ok(self.fill_model_bank()?)
    }

    // NOTE: this can probably stay, though I'm not sure that I want to deal with
    // delete buttons in the UI.

    // Returns Ok(Some(ModelId)) if the model was in the bank and there were no file errors
    // Returns Ok(None) if the model was not in the bank and there were no file errors
    // OR: if it's called on an asset-packed model.

    // Returns Err when there are issues with file removal, not including missing files:
    // - Since the file was to be removed anyway, it's not an error if it's missing and the map
    //   should be updated regardless.
    fn handle_removal(
        &self,
        model_id: ModelId,
        swap: bool,
    ) -> Result<Option<ModelId>, RibbleWhisperError> {
        let mut write_guard = self.model_map.write();
        if let Some(model) = write_guard.get(&model_id) {
            match model {
                ModelFile::Packed(_) => {
                    return Ok(None);
                }
                ModelFile::File(path) => {
                    // First check to see that it exists in the file_system before removing.
                    // To avoid two lookups, just manually check here.
                    match fs::remove_file(self.model_directory.join(path.as_ref())) {
                        Ok(_) => {}
                        Err(e) => match e.kind() {
                            ErrorKind::NotFound => {}
                            _ => return Err(RibbleWhisperError::IOError(e)),
                        },
                    };
                }
            }
        };

        if swap {
            Ok({
                write_guard
                    .swap_remove(&model_id).map(|_| model_id)
            })
        } else {
            Ok({
                write_guard
                    .shift_remove(&model_id).map(|_| model_id)
            })
        }
    }
}

pub struct RibbleModelBank {
    inner: Arc<RibbleModelBankState>,
    work_sender: Sender<WorkRequest>,
    download_sender: Sender<DownloadRequest>,
    worker_thread: Option<JoinHandle<()>>,
    debouncer: Option<Debouncer<RecommendedWatcher, RecommendedCache>>,
}

impl RibbleModelBank {
    // NOTE: this expects a fully canonicalized path.
    // This is handled higher up in the kernel implementation, but other users must uphold this
    // expectation.
    pub fn new(model_directory: &Path, bus: &Bus) -> Result<Self, RibbleError> {
        let (event_sender, event_receiver) = get_channel(SMALL_UTILITY_QUEUE_SIZE);
        let inner = Arc::new(RibbleModelBankState::new(model_directory, event_receiver)?);
        let mut debouncer = new_debouncer(
            std::time::Duration::from_millis(MAX_DEBOUNCE_TIME),
            None,
            event_sender,
        )?;


        let thread_inner = Arc::clone(&inner);
        let file_directory = model_directory.to_path_buf();

        // NOTE: the debouncer needs to be stored -> it will close on drop,
        // but since the inner thread is joined and blocks on the debouncer queue, it needs to be
        // stopped either by dropping or explicitly.
        debouncer.watch(file_directory.as_path(), RecursiveMode::NonRecursive)
            .expect("The debouncer is expected to watch without any issues.");

        let work_thread = std::thread::spawn(move || {
            while let Ok(result) = thread_inner.model_directory_watcher.recv() {
                match result {
                    Ok(events) => {
                        thread_inner.handle_debounced_events(&events);
                    }
                    // NOTE: at the moment, this is not being treated as a "fatal error",
                    // because there is no way to restart the debouncer yet and only the background
                    // thread will panic.
                    //
                    // (If this is to fail, it's more than likely to be a directory error which
                    // would trigger higher up in the app--but monitor logs just in-case)
                    Err(mut e) => {
                        while let Some(err) = e.pop() {
                            log::error!(
                                "Debouncer Error in model bank.\nError: {}\nError source: {:#?}",
                                err,
                                err.source()
                            );
                        }
                    }
                }
            }
        });

        // If the worker thread has already panicked/failed, this join should be very quick
        // and the model bank should fail to construct.
        if work_thread.is_finished() {
            let res = work_thread.join().map_err(|e| {
                RibbleError::ThreadPanic(format!("Failed to create debouncer thread.\n\
                Error: {e:#?}"))
            });

            return match res {
                Ok(_) => {
                    let err = RibbleError::Core("Debouncer thread quit early".to_string());
                    Err(err)
                }
                Err(e) => {
                    Err(e)
                }
            };
        }

        Ok(Self {
            inner,
            work_sender: bus.work_request_sender(),
            download_sender: bus.download_request_sender(),
            worker_thread: Some(work_thread),
            debouncer: Some(debouncer),
        })
    }

    pub fn model_directory(&self) -> &Path {
        self.inner.model_directory()
    }

    pub fn contains_model(&self, model_id: ModelId) -> bool {
        self.inner.contains_model(model_id)
    }

    // TODO: decide whether or not to allow users to set an alternative directory for models.
    // If that becomes a necessary feature, move to ArcSwap and set up logic for re-spawning the
    // watcher thread -> it's best to leave model management up to the user if they're already
    // swapping directories.

    pub fn download_new_model(&self, url: &str) {
        let model_directory = self.inner.model_directory();
        let download_request = DownloadRequest::new_job(url, model_directory);
        if let Err(e) = self.download_sender.try_send(download_request) {
            log::warn!(
                "Cannot make download request, channel either closed or too small.\n\
                Error: {}\n\
                Error source: {:#?}",
                &e,
                e.source()
            );
        }
    }

    // This will make a work request to copy the file over.
    pub fn copy_model_to_bank(&self, model_file_path: PathBuf) {
        let model_directory = self.inner.model_directory().to_path_buf();
        let worker = std::thread::spawn(move || {
            if !model_file_path.is_file() {
                let err =
                    RibbleError::Core(format!("{:#?} is not a file.", model_file_path.display()));
                return Err(err);
            }

            let extension = model_file_path
                .extension()
                .ok_or(RibbleError::Core(format!(
                    "Invalid file: {:#?}",
                    model_file_path.display()
                )))?;

            if extension != MODEL_FILE_EXTENSION {
                let err =
                    RibbleError::Core(format!("Invalid file_type: {:#?}", extension.display()));
                return Err(err);
            }

            let file_name = model_file_path
                .file_name()
                .ok_or(RibbleError::Core(format!(
                    "Invalid file path: {:#?}",
                    model_file_path.display()
                )))?;

            let dest = model_directory.join(file_name);

            fs::copy(model_file_path.as_path(), dest.as_path())?;
            let console_message = ConsoleMessage::Status(format!(
                "Saved model: {:#?} to models directory.",
                file_name.display()
            ));
            let ribble_message = RibbleMessage::Console(console_message);

            Ok(ribble_message)
        });

        let work_request = WorkRequest::Short(worker);

        if let Err(e) = self.work_sender.try_send(work_request) {
            log::warn!(
                "Failed to send work request. Channel may be closed or too small.\n\
                Error: {e}\n\
                Error source: {:#?}",
                e.source()
            );
        }
    }

    pub fn try_read_model_list(&self, copy_buffer: &mut Vec<(ModelId, ModelFile)>) {
        if let Some(guard) = self.inner.model_map.try_read() {
            copy_buffer.clear();
            copy_buffer.extend(guard.iter().map(|(k, v)| (*k, v.clone())));
        }
    }
}

impl Drop for RibbleModelBank {
    fn drop(&mut self) {
        log::info!("Dropping RibbleModelBank.");
        if let Some(debouncer) = self.debouncer.take() {
            log::info!("Stopping debouncer.");
            debouncer.stop();
        }
        if let Some(handle) = self.worker_thread.take() {
            log::info!("Joining RibbleModelBank Debouncer thread.");
            handle
                .join()
                .expect("Debouncer thread is expected to work properly and should not panic.");
            log::info!("RibbleModelBank Debouncer thread joined.");
        }
    }
}

impl ModelRetriever for RibbleModelBank {
    fn retrieve_model(&self, model_id: ModelId) -> Option<ModelLocation> {
        self.inner.retrieve_model(model_id)
    }
}
