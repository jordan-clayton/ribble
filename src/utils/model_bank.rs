use crate::utils::errors::RibbleError;
use case_style::CaseStyle;
use indexmap::IndexMap;
use parking_lot::RwLock;
use ribble_whisper::utils::errors::RibbleWhisperError;
use ribble_whisper::whisper::model::{ConcurrentModelBank, Model, ModelId, ModelRetriever};
use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use twox_hash::XxHash3_64;

// TODO: methods for downloading models (take in a bus to send download jobs).
// NOTE: this should also spawn jobs in the worker engine for COPY OPERATIONS ONLY.
// There will be a background thread to watch the directory.
//
// TODO TWICE: I'm not sure downloading should really happen here, copy yes, download no.
// Instead, maybe make it a kernel method.

// TODO: explicit methods for copying files over by path to create a model.
// Methods for to retrieve file_name from url slug/file path.
//
// TODO THRICE: Change visibility and move to kernel module; this isn't really a utility anymore.

const MODEL_ID_SEED: u64 = 0;
const MODEL_FILE_EXTENSION: &'static str = "bin";

struct RibbleModelBankState {
    model_directory: PathBuf,
    model_map: RwLock<IndexMap<ModelId, Model>>,
}

impl RibbleModelBankState {
    const DEFAULT_MODEL_MAP_SIZE: usize = 8;

    pub fn new(model_directory: &Path) -> Result<Self, RibbleError> {
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
                    entry
                        .path()
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_string(),
                    entry.metadata().unwrap().len(),
                )
            })
            .collect::<Vec<_>>();

        // Sort by file size.
        entries.sort_by(|(_, size1), (_, size2)| size1.cmp(size2));

        // Get a write lock to fill the map.
        let mut map_lock = self.model_map.write();

        for (file_name, _) in entries {
            let model_key = XxHash3_64::oneshot_with_seed(MODEL_ID_SEED, file_name.as_bytes());
            let stripped_file_name = file_name.replace(".bin", "").replace(".en", "-en");
            let name = CaseStyle::guess(&stripped_file_name)
                .unwrap_or(CaseStyle::from_kebabcase(&stripped_file_name))
                .to_pascalcase();
            let model = Model::new().with_name(name).with_file_name(file_name);

            map_lock.insert(model_key, model);
        }

        Ok(())
    }

    // NOTE: this assumes that the model's got a fully canonicalized path and probably shouldn't be
    // called directly.
    //
    // The trait method really isn't the best way to handle this; expose a public method on
    // RibbleModelBank that takes in a model path and handles all file_name/name logic.
    //
    fn insert_model(&self, model: Model) -> Result<ModelId, RibbleWhisperError> {
        let given_file_path = PathBuf::from(model.file_name());
        if given_file_path.extension().is_none_or(|ext| ext != "bin") {
            return Err(RibbleWhisperError::ParameterError(format!(
                "Model: {}, has an invalid path: {}",
                model.name(),
                model.file_name()
            )));
        }

        let file_name = given_file_path
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| Some(s.to_string()))
            .ok_or(RibbleWhisperError::ParameterError(format!(
                "Model: {}, has an invalid path: {}",
                model.name(),
                model.file_name()
            )))?;

        // Make the file copy.
        fs::copy(given_file_path, self.model_directory.join(&file_name))?;
        let model_id = XxHash3_64::oneshot_with_seed(MODEL_ID_SEED, file_name.as_bytes());
        let insert_model = model.with_file_name(file_name);
        self.model_map.write().insert(model_id, insert_model);
        Ok(model_id)
    }

    fn get_model(&self, model_id: ModelId) -> Option<Model> {
        self.model_map
            .read()
            .get(&model_id)
            .and_then(|model| Some(model.clone()))
    }

    fn rename_model(
        &self,
        model_id: ModelId,
        new_name: String,
    ) -> Result<Option<ModelId>, RibbleWhisperError> {
        if let Some(model) = self.model_map.write().get_mut(&model_id) {
            model.rename(new_name);
            Ok(Some(model_id))
        } else {
            Ok(None)
        }
    }

    fn change_model_file_name(
        &self,
        model_id: ModelId,
        new_file_path: String,
    ) -> Result<Option<ModelId>, RibbleWhisperError> {
        // Check for an existing model and clone it to set the new file path -> this assumes the
        // full qualified path is being passed and the file is being copied over into the model directory.
        let model = self
            .model_map
            .read()
            .get(&model_id)
            .and_then(|model| Some(model.clone().with_file_name(new_file_path)));

        // If there exists an old model to clone over (with the copy path), insert it at the end of the buffer.
        // Then, swap the old in-memory model out for the new one to keep relative position.
        if let Some(model) = model {
            let new_model_id = self.insert_model(model)?;
            // Do a swap-remove so that it takes the old spot.
            // If the new file size is larger than the last one, this order will be resolved on a full-refresh/app load.
            self.handle_removal(model_id, true)?;
            Ok(Some(new_model_id))
        } else {
            Ok(None)
        }
    }

    fn model_exists_in_storage(&self, model_id: ModelId) -> Result<bool, RibbleWhisperError> {
        if let Some(model) = self.model_map.read().get(&model_id) {
            let full_path = self.model_directory.join(model.file_name());
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

    fn clear(&self) {
        self.model_map.write().clear();
    }

    fn refresh_model_bank(&self) -> Result<(), RibbleWhisperError> {
        self.clear();
        Ok(self.fill_model_bank()?)
    }

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
            match fs::remove_file(self.model_directory.join(&model.file_name())) {
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
    // I'm not sure whether this should be here, or whether this should be handled by the kernel.
    download_sender: Sender<DownloadRequest>,
}

impl RibbleModelBank {
    pub(crate) fn new(model_directory: &Path, bus: &Bus) -> Result<Self, RibbleError> {
        let inner = Arc::new(RibbleModelBankState::new(model_directory)?);
        // TODO: watcher thread.
        todo!("Move module, fix imports, and write the watcher thread.");
        Ok(Self { inner })
    }

    // NOTE: since for_each in the ConcurrentModelBank only takes Fn(..), it's not possible to draw UI code with the egui
    // cursor.
    // Until there's a genuine need in ribble_whisper for FnMut((&ModelId, &Model)), just use this
    // instead for drawing UI.
    pub(crate) fn for_each_mut_capture<F: FnMut((&ModelId, &Model))>(&self, mut f: F) {
        // Call this if the user has opened the model's data directory from the app.
        for (model_id, model) in self.inner.model_map.read().iter() {
            f((model_id, model));
        }
    }

    // This will make a work request to copy the file over.
    pub(super) fn copy_model_path(&self, model_file_path: &Path) {
        let model_directory = self.model_directory();
        let worker = std::thread::spawn(move || {
            if !model_file_path.is_file() {
                let err = RibbleError::Core(format!("{:?} is not a file.", model_file_path));
                return Err(err);
            }

            let extension = model_file_path
                .extension()
                .ok_or(RibbleError::Core(format!(
                    "Invalid file: {:?}",
                    model_file_path
                )))?;

            if extension != MODEL_FILE_EXTENSION {
                let err = RibbleError::Core(format!("Invalid file_type: {:?}", extension));
                return Err(err);
            }

            let file_name = model_file_path
                .file_name()
                .ok_or(RibbleError::Core(format!(
                    "Invalid file path: {:?}",
                    model_file_path
                )))?;

            let dest = model_directory.join(file_name);

            std::fs::copy(model_file_path, dest.as_path())?;

            let ribble_message = RibbleMessage::Console();

            todo!()
        });

        let work_request = WorkRequest::Short(worker);
        // TODO: determine whether or not it's safe to block -- it shouuuuld be, but I'm not sure.
        if self.work_sender.try_send(work_request).is_err() {
            todo!("LOGGING");
        }
    }
}

impl Drop for RibbleModelBank {
    fn drop(&mut self) {
        todo!()
    }
}

impl ConcurrentModelBank for RibbleModelBank {
    fn model_directory(&self) -> &Path {
        self.inner.model_directory()
    }

    // NOTE: This method really shouldn't be called directly unless the Model has a canonicalized
    // path.
    //
    // Prefer using [todo: public method that takes a file path] instead of this one directly.
    fn insert_model(&self, model: Model) -> Result<ModelId, RibbleWhisperError> {
        self.inner.insert_model(model)
    }

    fn model_exists_in_storage(&self, model_id: ModelId) -> Result<bool, RibbleWhisperError> {
        self.inner.model_exists_in_storage(model_id)
    }

    fn get_model(&self, model_id: ModelId) -> Option<Model> {
        self.inner.get_model(model_id)
    }

    // This tries to fetch a model and update its  (user-facing) name.
    // # Returns:
    // * Ok(Some(ModelId)) -> the ModelId for the updated model, on success.
    // * Ok(None) -> the old Model was not in the bank, if the id wasn't present
    fn rename_model(
        &self,
        model_id: ModelId,
        new_name: String,
    ) -> Result<Option<ModelId>, RibbleWhisperError> {
        self.inner.rename_model(model_id, new_name)
    }

    // This changes out the file name if an old entry exists in the bank.
    // Equivalently, this should be used for restoring a missing file if the key exists in the bank
    // without also being on disk.
    // # Returns:
    // * Ok(ModelId) -> the new ModelId for the updated model
    // * Ok(None) -> the old Model was not in the bank
    // * Err(RibbleWhisperError) -> File IO error.

    // If it is the case that a user manages to delete an entry from the hashmap without removing
    // the physical file, the entry will appear when the folder is refreshed/the app launches.
    // It's not possible with this implementation to locate a model if the key-value pair is missing.

    fn change_model_file_name(
        &self,
        model_id: ModelId,
        new_file_path: String,
    ) -> Result<Option<ModelId>, RibbleWhisperError> {
        self.inner.change_model_file_name(model_id, new_file_path)
    }

    // NOTE: I'm not sure that this is something that is really worth exposing in the GUI.
    // Instead, expose the directory in the host's file manager and let them handle deletion
    // The watcher will ensure that the model bank is up-to-date.
    //
    // It is not an error to call this method, but it will block so keep that in mind.
    fn remove_model(&self, model_id: ModelId) -> Result<Option<ModelId>, RibbleWhisperError> {
        self.inner.handle_removal(model_id, false)
    }

    // Refreshes the model bank by clearing the internal map and re-running the directory crawler.
    //
    // This probably shouldn't be exposed in the GUI.
    fn refresh_model_bank(&self) -> Result<(), RibbleWhisperError> {
        self.inner.refresh_model_bank()
    }

    fn for_each<F>(&self, f: F)
    where
        F: Fn((&ModelId, &Model)),
    {
        for (model_id, model) in self.inner.model_map.read().iter() {
            f((model_id, model));
        }
    }
}

impl ModelRetriever for RibbleModelBank {
    fn retrieve_model_path(&self, model_id: ModelId) -> Option<PathBuf> {
        self.inner
            .model_map
            .read()
            .get(&model_id)
            .and_then(|model| Some(self.model_directory().join(model.file_name())))
    }
}
