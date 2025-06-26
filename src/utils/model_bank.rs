use crate::utils::errors::RibbleError;
use ribble_whisper::utils::errors::RibbleWhisperError;
use ribble_whisper::whisper::model::{ConcurrentModelBank, Model, ModelId, ModelRetriever};
use scc::ebr::Guard;
use scc::HashIndex;
use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

// TODO: some sort of serialized hashmap <Model file path, Model Name>
// Possibly write on drop.
struct RibbleModelBankState {
    id_accumulator: AtomicU64,
    model_directory: PathBuf,
    model_map: HashIndex<ModelId, Model>,
}

impl RibbleModelBankState {
    const DEFAULT_MODEL_MAP_SIZE: usize = 8;
    // TODO: should probably return Result<Self, RibbleWhisperError> if the wrong directory has been passed.
    pub fn new(model_directory: &Path) -> Result<Self, RibbleError> {
        let model_map = HashIndex::with_capacity(Self::DEFAULT_MODEL_MAP_SIZE);
        let id_accumulator = AtomicU64::new(0);
        let model_directory = model_directory.to_path_buf();

        // Test to make sure model_directory exists and is.
        if !fs::metadata(&model_directory)?.is_dir() {
            Err(RibbleError::Core(format!(
                "Model path: {:?} is not a directory",
                model_directory
            )))
        } else {
            Self {
                id_accumulator,
                model_directory,
                model_map,
            }
                .init()
        }
    }
    fn init(self) -> Result<Self, RibbleError> {
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
            .map(|entry| (entry.path().file_name().unwrap().to_str().unwrap().to_string(), entry.metadata().unwrap().len()))
            .collect::<Vec<_>>();

        // Sort by file size.
        entries.sort_by(|(_, size1), (_, size2)| size2.cmp(size1));
        // TODO: figure out how to get the stored short-names, use the file_name as fallback (strip the extension)

        for (file_name, _) in entries {
            let key = self.id_accumulator.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            let model = Model::new()
                .with_name("TODO: FIGURE OUT HOW TO RESOLVE THIS".to_string())
                .with_file_name(file_name);

            if let Err(e) = self.model_map.insert(key, model) {
                // TODO: logging -> this -should- never ever happen.
            }
        }
        Ok(self)
    }
}

pub(crate) trait GetSharedModelRetriever<MR: ModelRetriever> {
    fn get_model_retriever(&self) -> Arc<MR>;
}
impl ConcurrentModelBank for RibbleModelBankState {
    fn model_directory(&self) -> &Path {
        self.model_directory.as_path()
    }

    // Not 100% sold on the implementation as it exists here -> the UI should only really pass
    // the name + file_path to "load" a file. Exposing the data directory is probably a bad idea, but
    // not entirely unsound--just expensive to re-crawl the system.
    // It would be easier to handle the copy operation internally.

    // *** Going impl: Assume the model being passed in has the full-path for its file name
    fn insert_model(&self, model: Model) -> Result<ModelId, RibbleWhisperError> {
        // TODO: also manage short-name/file_path incoherence, or
        todo!();
    }

    // NOTE: As per the going implementation, unless full path copying is handled elsewhere,
    // assume it involves a full file path swap and copy.
    fn replace_model(
        &self,
        model_id: ModelId,
        model: Model,
    ) -> Result<ModelId, RibbleWhisperError> {
        // Get the old file from the map:
        // If it exists, check the directory for the old file and uh... remove it? And then swap the hashmap entry
        // If it doesn't exist, just insert it into the hashmap and copy it over to disk.
        todo!()
    }

    fn update_model_parameters(
        &self,
        _model_id: ModelId,
        _name: Option<String>,
        _file_name: Option<String>,
    ) -> Result<ModelId, RibbleWhisperError> {
        unreachable!(
            "This method should never be called unless it does actually need to be implemented."
        )
    }

    fn model_exists_in_storage(&self, model_id: ModelId) -> Result<bool, RibbleWhisperError> {
        if !self.model_map.contains(&model_id) {
            Ok(false)
        } else {
            let guard = Guard::new();
            let model = self.model_map.peek(&model_id, &guard).ok_or(
                RibbleWhisperError::ParameterError(format!("Invalid model id: {}", model_id))
            )?;

            let full_path = self.model_directory.join(model.file_name());
            let metadata = fs::metadata(&full_path);
            match metadata {
                Ok(m) => { Ok(m.is_file()) }
                Err(e) => {
                    match e.kind() {
                        ErrorKind::NotFound => Ok(false),
                        _ => Err(RibbleWhisperError::IOError(e))
                    }
                }
            }
        }
    }

    fn retrieve_model(&self, model_id: ModelId) -> Option<&Model> {
        self.model_map.get(&model_id).and_then(|entry| Some(entry.get()))
    }

    fn remove_model(&self, model_id: ModelId) -> Result<ModelId, RibbleWhisperError> {
        todo!()
    }
}

impl ModelRetriever for RibbleModelBankState {
    fn retrieve_model_path(&self, model_id: ModelId) -> Option<PathBuf> {
        let guard = Guard::new();
        self.model_map.peek(&model_id, &guard).and_then(|model| {
            Some(self.model_directory.join(model.file_name()))
        })
    }
}

pub(crate) struct RibbleModelBank {
    inner: Arc<RibbleModelBankState>,
}

impl RibbleModelBank {
    pub(crate) fn new(model_directory: &Path) -> Self {
        todo!("")
    }
}

impl<MR: ModelRetriever> GetSharedModelRetriever<MR> for RibbleModelBank {
    fn get_model_retriever(&self) -> Arc<MR> {
        Arc::clone(&self.inner)
    }
}

impl ConcurrentModelBank for RibbleModelBank {
    fn model_directory(&self) -> &Path {
        self.inner.model_directory()
    }

    fn insert_model(&self, model: Model) -> Result<ModelId, RibbleWhisperError> {
        self.inner.insert_model(model)
    }

    fn replace_model(
        &self,
        model_id: ModelId,
        model: Model,
    ) -> Result<ModelId, RibbleWhisperError> {
        self.inner.replace_model(model_id, model)
    }

    // TODO: this is probably not the way to go -> HashIndex allows for an owned handle to the inner data, so it can be consumed/replaced
    fn update_model_parameters(
        &self,
        _model_id: ModelId,
        _name: Option<String>,
        _file_name: Option<String>,
    ) -> Result<ModelId, RibbleWhisperError> {
        unreachable!(
            "This method should never be called unless it should--then it needs to be implemented."
        )
    }

    fn model_exists_in_storage(&self, model_id: ModelId) -> Result<bool, RibbleWhisperError> {
        self.inner.model_exists_in_storage(model_id)
    }

    fn retrieve_model(&self, model_id: ModelId) -> Option<&Model> {
        self.inner.retrieve_model(model_id)
    }

    fn remove_model(&self, model_id: ModelId) -> Result<ModelId, RibbleWhisperError> {
        self.inner.remove_model(model_id)
    }
}
