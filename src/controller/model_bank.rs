use crate::controller::{
    Bus, ConsoleMessage, DownloadRequest, RibbleMessage, SMALL_UTILITY_QUEUE_SIZE, WorkRequest,
};
use crate::utils::errors::RibbleError;
use indexmap::IndexMap;
use parking_lot::RwLock;
use ribble_whisper::utils::errors::RibbleWhisperError;
use ribble_whisper::utils::{Receiver, Sender, get_channel};
use ribble_whisper::whisper::model::{ModelId, ModelRetriever};
use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use twox_hash::XxHash3_64;

use notify_debouncer_full::notify::EventKind;
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, new_debouncer, notify::RecursiveMode,
};

const MODEL_ID_SEED: u64 = 0;
const MODEL_FILE_EXTENSION: &'static str = "bin";
// NOTE: THIS IS IN MILLISECONDS
// NOTE TWICE: if this is burning cycles, increase the timeout or add an explicit tick-rate that's
// close to the actual timeout.
const MAX_DEBOUNCE_TIME: u64 = 2000;

struct RibbleModelBankState {
    model_directory: PathBuf,
    // NOTE: this isn't using ribble_whisper's model/ConcurrentModelBank abstraction.
    // It's way, way easier and cheaper to use Arc<str>
    model_map: RwLock<IndexMap<ModelId, Arc<str>>>,
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
                format!("Model path: {:?} is not a directory", model_directory),
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

        if self.refresh_model_bank().is_err() {
            todo!("LOGGING");
        }
    }

    fn fill_model_bank(&self) -> Result<(), std::io::Error> {
        let data_directory = &self.model_directory;
        // TODO: possibly clean this up -it's a little wild to try and read.
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
            .map(|entry| {
                (
                    Arc::from(entry.path().file_name().unwrap().to_str().unwrap()),
                    entry.metadata().unwrap().len(),
                )
            })
            .collect::<Vec<(Arc<str>, u64)>>();

        // Sort by file size.
        entries.sort_by(|(_, size1), (_, size2)| size1.cmp(size2));

        // Get a write lock to fill the map.
        let mut map_lock = self.model_map.write();

        for (file_name, _) in entries {
            let model_key = XxHash3_64::oneshot_with_seed(MODEL_ID_SEED, file_name.as_bytes());
            map_lock.insert(model_key, file_name);
        }

        Ok(())
    }

    fn get_model(&self, model_id: ModelId) -> Option<Arc<str>> {
        self.model_map
            .read()
            .get(&model_id)
            .and_then(|model| Some(Arc::clone(model)))
    }

    fn contains_model(&self, model_id: ModelId) -> bool {
        self.model_map.read().contains_key(&model_id)
    }

    fn retrieve_model_path(&self, model_id: ModelId) -> Option<PathBuf> {
        self.model_map
            .read()
            .get(&model_id)
            .and_then(|model| Some(self.model_directory().join(model.as_ref())))
    }

    // NOTE: this code can probably stay, though it should be impossible for this to return false based on
    // the directory crawler.
    fn model_exists_in_storage(&self, model_id: ModelId) -> Result<bool, RibbleWhisperError> {
        if let Some(model) = self.model_map.read().get(&model_id) {
            let full_path = self.model_directory.join(model.as_ref());
            let metadata = fs::metadata(&full_path);
            match metadata {
                Ok(m) => Ok(m.is_file()),
                Err(e) => match e.kind() {
                    ErrorKind::NotFound => Ok(false),
                    _ => Err(RibbleWhisperError::IOError(e)),
                },
            }
        } else {
            Ok(false)
        }
    }

    // These two methods can stay.
    fn clear(&self) {
        self.model_map.write().clear();
    }

    fn refresh_model_bank(&self) -> Result<(), RibbleWhisperError> {
        self.clear();
        Ok(self.fill_model_bank()?)
    }

    // NOTE: this can probably stay, though I'm not sure I want to deal with delete buttons in the
    // UI.
    // Returns Ok(Some(ModelId)) if the model was in the bank and there were no file errors
    // Returns Ok(None) if the model was not in the bank and there were no file errors
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
            // First check to see that it exists in the file_system before removing.
            // To avoid two lookups, just manually check here.
            match fs::remove_file(self.model_directory.join(model.as_ref())) {
                Ok(_) => {}
                Err(e) => match e.kind() {
                    ErrorKind::NotFound => {}
                    _ => return Err(RibbleWhisperError::IOError(e)),
                },
            };
        };

        if swap {
            Ok({
                write_guard
                    .swap_remove(&model_id)
                    .and_then(|_| Some(model_id))
            })
        } else {
            Ok({
                write_guard
                    .shift_remove(&model_id)
                    .and_then(|_| Some(model_id))
            })
        }
    }
}

pub(crate) struct RibbleModelBank {
    inner: Arc<RibbleModelBankState>,
    // TODO: thread handle + watcher.
    work_sender: Sender<WorkRequest>,
    download_sender: Sender<DownloadRequest>,
    worker_thread: Option<JoinHandle<Result<(), RibbleError>>>,
}

impl RibbleModelBank {
    // NOTE: this filename should be canonicalized before it's passed in here
    // CHECK THIS IN THE CONTROLLER CONSTRUCTOR AND ESCAPE EARLY.
    pub(crate) fn new(model_directory: &Path, bus: &Bus) -> Result<Self, RibbleError> {
        let (event_sender, event_receiver) = get_channel(SMALL_UTILITY_QUEUE_SIZE);
        let inner = Arc::new(RibbleModelBankState::new(model_directory, event_receiver)?);
        // TODO: watcher thread -> watch the directory for create/modify/delete and just run the update routine.

        // TODO: this will need a hooooook for errors in RibbleError
        // Args: timeout, tick-rate (None => 1/4 * timeout), event_handler (queue)
        let mut debouncer = new_debouncer(
            std::time::Duration::from_millis(MAX_DEBOUNCE_TIME),
            None,
            event_sender,
        )?;

        let thread_inner = Arc::clone(&inner);
        let file_directory = model_directory.to_path_buf();
        let work_thread = std::thread::spawn(move || {
            debouncer.watch(file_directory.as_path(), RecursiveMode::NonRecursive)?;

            while let Ok(result) = thread_inner.model_directory_watcher.recv() {
                match result {
                    Ok(events) => {
                        thread_inner.handle_debounced_events(&events);
                    }
                    Err(mut e) => {
                        // TODO: LOGGING
                        // TODO TWICE: determine whether or not this should return the result.
                        // TODO THRICE: determine whether this is considered "panic" territory, or
                        // whether the app should continue running/optional re-spawn the thread.
                        // Most errors seem pretty dire (can't watch/io/OS specific), but a
                        // "refresh" button for manual directory crawling mightn't be so bad--
                        // though this is most likely to encounter fs issues if that's true anyway.
                        let mut last_err = None;
                        while let Some(err) = e.pop() {
                            // TODO: LOGGING
                            last_err = Some(err);
                        }

                        if let Some(err) = last_err.take() {
                            return Err(err.into());
                        }
                    }
                }
            }

            Ok(())
        });

        Ok(Self {
            inner,
            work_sender: bus.work_request_sender(),
            download_sender: bus.download_request_sender(),
            worker_thread: Some(work_thread),
        })
    }

    pub(crate) fn model_directory(&self) -> &Path {
        self.inner.model_directory()
    }

    pub(crate) fn contains_model(&self, model_id: ModelId) -> bool {
        self.inner.contains_model(model_id)
    }

    // TODO: decide whether or not to allow users to set an alternative directory for models.
    // If that becomes a necessary feature, move to ArcSwap and set up logic for re-spawning the
    // watcher thread -> it's best to leave model management up to the user if they're already
    // swapping directories.

    pub(crate) fn download_new_model(&self, url: &str) {
        let model_directory = self.inner.model_directory();
        let download_request = DownloadRequest::new_with_params(url, model_directory);
        if self.download_sender.send(download_request).is_err() {
            todo!("LOGGING: download issue.");
        }
    }

    // This will make a work request to copy the file over.
    pub(crate) fn copy_model_to_bank(&self, model_file_path: PathBuf) {
        let model_directory = self.inner.model_directory().to_path_buf();
        let worker = std::thread::spawn(move || {
            if !model_file_path.is_file() {
                let err =
                    RibbleError::Core(format!("{:?} is not a file.", model_file_path.display()));
                return Err(err);
            }

            let extension = model_file_path
                .extension()
                .ok_or(RibbleError::Core(format!(
                    "Invalid file: {:?}",
                    model_file_path.display()
                )))?;

            if extension != MODEL_FILE_EXTENSION {
                let err = RibbleError::Core(format!("Invalid file_type: {:?}", extension));
                return Err(err);
            }

            let file_name = model_file_path
                .file_name()
                .ok_or(RibbleError::Core(format!(
                    "Invalid file path: {:?}",
                    model_file_path.display()
                )))?;

            let dest = model_directory.join(file_name);

            fs::copy(model_file_path.as_path(), dest.as_path())?;
            let console_message = ConsoleMessage::Status(format!(
                "Saved model: {:#?} to models directory.",
                file_name
            ));
            let ribble_message = RibbleMessage::Console(console_message);

            Ok(ribble_message)
        });

        let work_request = WorkRequest::Short(worker);
        // TODO: determine whether or not it's safe to block -- it shouuuuld be, but I'm not sure.
        if self.work_sender.try_send(work_request).is_err() {
            todo!("LOGGING");
        }
    }

    pub(crate) fn try_read_model_list(&self, copy_buffer: &mut Vec<(ModelId, Arc<str>)>) {
        if let Some(guard) = self.inner.model_map.try_read() {
            copy_buffer.clear();
            copy_buffer.extend(guard.iter().map(|(k, v)| (*k, Arc::clone(v))));
        }
    }
}

impl Drop for RibbleModelBank {
    fn drop(&mut self) {
        if let Some(handle) = self.worker_thread.take() {
            if handle.join().is_err() {
                todo!("LOGGING")
            }
        }
    }
}

impl ModelRetriever for RibbleModelBank {
    fn retrieve_model_path(&self, model_id: ModelId) -> Option<PathBuf> {
        self.inner.retrieve_model_path(model_id)
    }
}
