use crate::utils::errors::RibbleError;
use case_style::CaseStyle;
use indexmap::map::Iter;
use indexmap::IndexMap;
use parking_lot::{RwLock, RwLockReadGuard};
use ribble_whisper::utils::errors::RibbleWhisperError;
use ribble_whisper::whisper::model::{ConcurrentModelBank, Model, ModelId, ModelRetriever};
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io::{BufReader, BufWriter, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::panicking;
use twox_hash::XxHash3_64;

// TODO: since this has -very- little state to deal with, and there's some pointer
// chasing involved already to work with RealtimeTranscriber, this should be moved
// back into RibbleModelBank; the inner pointer is absolutely unnecessary.
struct RibbleModelBankState {
    model_directory: PathBuf,
    model_map: RwLock<IndexMap<ModelId, Model>>,
}

impl RibbleModelBankState {
    // TODO: maybe don't include the file extension and dynamically generate if using different serializer.
    const MODEL_SHORT_NAMES_MAP: &'static str = "model_names.ron";
    const DEFAULT_MODEL_MAP_SIZE: usize = 8;

    // TODO: pick an actual seed value, can even just be zero.
    const MODEL_ID_SEED: u64 = 0;
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

    fn clear(&self) {
        self.model_map.write().clear();
    }

    fn handle_removal(
        &self,
        model_id: ModelId,
        swap: bool,
    ) -> Result<Option<Model>, RibbleWhisperError> {
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
            Ok(write_guard.swap_remove(&model_id))
        } else {
            Ok(write_guard.shift_remove(&model_id))
        }
    }

    fn model_map_as_iter(&self) -> Iter<ModelId, Model> {
        self.model_map.read().iter()
    }

    fn fill_model_bank(&self) -> Result<(), RibbleError> {
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

        let prev_map = self.deserialize_model_map().unwrap_or_default();

        let mut map_lock = self.model_map.write();
        for (file_name, _) in entries {
            let model_key =
                XxHash3_64::oneshot_with_seed(Self::MODEL_ID_SEED, file_name.as_bytes());
            // Get the old model name if it's there, clone it over with the new file_name
            let model = if let Some(model_name) = prev_map.get(&file_name) {
                Model::new()
                    .with_name(model_name.to_string())
                    .with_file_name(file_name)
            } else {
                let stripped_file_name = file_name.replace(".bin", "").replace(".en", "-en");
                let name = CaseStyle::guess(&stripped_file_name)
                    .unwrap_or(CaseStyle::from_kebabcase(&stripped_file_name))
                    .to_pascalcase();
                Model::new().with_name(name).with_file_name(file_name)
            };

            if let Err(_e) = map_lock.insert(model_key, model) {
                // TODO: logging -> this -should- never ever happen.
            }
        }

        Ok(())
    }

    // TODO: call this whenever running the serialize app state thread + on closing.
    fn serialize_model_map(&mut self) {
        let model_map = self
            .model_map
            .read()
            .iter()
            .map(|(_, v)| (v.file_name().to_string(), v.name().to_string()))
            .collect::<IndexMap<_, _>>();
        let model_ron = ron::ser::to_string(&model_map);
        let model_file_path = self.model_directory.join(Self::MODEL_SHORT_NAMES_MAP);
        let model_file = File::create(model_file_path);
        if let Ok((model_map, model_file)) = (model_ron, model_file) {
            let mut writer = BufWriter::new(model_file);
            if let Err(_e) = ron::ser::to_writer_pretty(&mut writer, model_map, Default::default()) {
                todo!("LOGGING!")
            }
        }
    }

    fn deserialize_model_map(&self) -> Option<IndexMap<String, String>> {
        let file_path = self.model_directory.join(Self::MODEL_SHORT_NAMES_MAP);
        match File::open(&file_path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                match ron::de::from_reader(reader) {
                    Ok(val) => Some(val),
                    Err(_) => None,
                }
            }
            Err(_) => None,
        }
    }
}

impl Drop for RibbleModelBankState {
    fn drop(&mut self) {
        if !panicking() {
            self.serialize_model_map();
        }
    }
}

// TODO! MOVE THIS TO SOMEWHERE AND THEN IMPLEMENT FOR RibbleModelBank + State -> Perhaps instead I rely on RAII semantics though, or both.
// Basically, anything that needs to be serialized upon app removal.
pub(crate) trait SerializableEntity {}

pub(crate) struct RibbleModelBankIter<'a> {
    read_guard: RwLockReadGuard<'a, IndexMap<ModelId, Model>>,
    inner: Iter<'a, ModelId, Model>,
}

impl<'a> Iterator for RibbleModelBankIter<'a> {
    type Item = (&'a ModelId, &'a Model);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl ConcurrentModelBank for RibbleModelBankState {
    // TODO: this very obviously will not compile, but it gets the point across.
    // Make a wrapper struct that implements Iterator that holds the write guard.
    type Iter<'a>
    = RibbleModelBankIter<'a>
    where
        Self: 'a;

    fn model_directory(&self) -> &Path {
        self.model_directory.as_path()
    }

    /// NOTE: this is expected to be passed in as absolute from another location.
    /// NOTE: CANONICALIZE THE FULL PATH BEFORE PASSING TO THIS METHOD.
    fn insert_model(&self, model: Model) -> Result<ModelId, RibbleWhisperError> {
        let given_file_path = PathBuf::from(model.file_name());
        if !given_file_path.extension() == Some("bin".as_ref()) {
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
        let model_id = XxHash3_64::oneshot_with_seed(Self::MODEL_ID_SEED, file_name.as_bytes());
        let insert_model = model.with_file_name(file_name);
        self.model_map.write().insert(model_id, insert_model);
        Ok(model_id)
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

    fn get_model(&self, model_id: ModelId) -> Option<&Model> {
        self.model_map.read().get(&model_id)
    }

    // Turns the IndexMap into an iterator that is guaranteed to live at least as long as RibbleModeBankState.
    fn iter(&self) -> Self::Iter<'_> {
        let read_guard = self.model_map.read();
        let inner = read_guard.iter();
        RibbleModelBankIter { read_guard, inner }
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
        if let Some(model) = self.model_map.write().get_mut(&model_id) {
            model.rename(new_name);
            Ok(Some(model_id))
        } else {
            Ok(None)
        }
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

    // Basic idea: Remove from the filesystem first; if there's an IO error that isn't a (File not found), return the error.
    // Otherwise, proceed with removing from the hashmap.
    // The file needs to be removed before removing from the map.
    // NOTE: there should be checks in the UI to prevent the user from running transcription with an old model
    // Removal is a shift_remove (moves all elements so order is preserved)
    fn remove_model(&self, model_id: ModelId) -> Result<Option<Model>, RibbleWhisperError> {
        self.handle_removal(model_id, false)
    }

    // Refreshes the model bank by clearing the internal map and re-running the directory crawler.
    // Call this if the user has opened the model's data directory from the app.
    fn refresh_model_bank(&self) -> Result<(), RibbleWhisperError> {
        self.clear();
        self.fill_model_bank().into()
    }
}

impl ModelRetriever for RibbleModelBankState {
    fn retrieve_model_path(&self, model_id: ModelId) -> Option<PathBuf> {
        self.model_map
            .read()
            .get(&model_id)
            .and_then(|model| Some(self.model_directory.join(model.file_name())))
    }
}

pub(crate) struct RibbleModelBank {
    inner: Arc<RibbleModelBankState>,
}

// TODO: Replace RibbleModelBank with its inner implementation.
impl RibbleModelBank {
    pub(crate) fn new(model_directory: &Path) -> Result<Self, RibbleError> {
        let inner = Arc::new(RibbleModelBankState::new(model_directory)?);
        Ok(Self { inner })
    }
}


impl ConcurrentModelBank for RibbleModelBank {
    type Iter<'a> = RibbleModelBankIter<'a>;

    fn model_directory(&self) -> &Path {
        self.inner.model_directory()
    }

    fn insert_model(&self, model: Model) -> Result<ModelId, RibbleWhisperError> {
        self.inner.insert_model(model)
    }

    fn model_exists_in_storage(&self, model_id: ModelId) -> Result<bool, RibbleWhisperError> {
        self.inner.model_exists_in_storage(model_id)
    }

    fn get_model(&self, model_id: ModelId) -> Option<&Model> {
        self.inner.get_model(model_id)
    }

    fn iter(&self) -> Self::Iter<'_> {
        self.inner.iter()
    }

    fn rename_model(
        &self,
        model_id: ModelId,
        new_name: String,
    ) -> Result<Option<ModelId>, RibbleWhisperError> {
        self.inner.rename_model(model_id, new_name)
    }

    fn change_model_file_name(
        &self,
        model_id: ModelId,
        new_file_name: String,
    ) -> Result<Option<ModelId>, RibbleWhisperError> {
        self.inner.change_model_file_name(model_id, new_file_name)
    }

    fn remove_model(&self, model_id: ModelId) -> Result<Option<Model>, RibbleWhisperError> {
        self.inner.remove_model(model_id)
    }

    fn refresh_model_bank(&self) -> Result<(), RibbleWhisperError> {
        self.inner.refresh_model_bank()
    }
}

impl ModelRetriever for RibbleModelBank {
    fn retrieve_model_path(&self, model_id: ModelId) -> Option<PathBuf> {
        self.inner.retrieve_model_path(model_id)
    }
}
