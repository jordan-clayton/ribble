use crate::ui::app::Ribble;
use crate::utils::crash_handler::set_up_desktop_crash_handler;
use crate::utils::errors::RibbleError;
use crate::utils::migration::{
    clear_old_ribble_state, migrate_model_filenames, RibbleVersion, Version,
};
use crash_handler::CrashHandler;
use directories::ProjectDirs;
use eframe::{run_native, AppCreator, NativeOptions};
use egui::{IconData, ViewportBuilder};
use flexi_logger::{
    Age, Cleanup, Criterion, Duplicate, FileSpec, Logger, LoggerHandle, Naming, WriteMode,
};
use image::GenericImageView;
use ron::ser::PrettyConfig;
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;

pub const APP_ID: &str = "Ribble";
pub const QUALIFIER: &str = "com";
pub const ORGANIZATION: &str = "Jordan";

#[cfg(not(target_os = "macos"))]
pub static ICON_BYTES: &[u8] = include_bytes!("assets/whisper_app_icon_128x128@1x.png");

#[cfg(target_os = "macos")]
pub static ICON_BYTES: &[u8] = include_bytes!("assets/whisper_app_icon_1024x1024@1x.png");

static MIGRATION_VERSION: OnceLock<RibbleVersion> = OnceLock::new();

// NOTE: if there's a significant change to the directory structures, things might
// get a little fragile -> try to keep this to a minimum if at all possible so that
// old paths don't need to be maintained, or move to a different Versioning mechanism.
const OLD_MODEL_STUB: &str = "models";

pub struct RibbleRunner<'a> {
    version: RibbleVersion,
    // NOTE: this -could- have just a path reference, perhaps that might be better.
    data_directory: PathBuf,
    app: Option<AppCreator<'a>>,
    window_options: Option<NativeOptions>,
    _logger: Option<LoggerHandle>,
    _crash_handler: CrashHandler,
}

impl RibbleRunner<'_> {
    const VERSION_FILE_NAME: &'static str = "version.ron";
    // TODO: if this constant needs to change, change it
    // Right now it's good for a "week's worth".
    const MAX_LOG_FILES: usize = 7;
    const LOG_FILE_NAME: &'static str = "ribble_log";

    pub fn new() -> Result<Self, RibbleError> {
        // Set up the project directory
        let proj_dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, APP_ID).ok_or(
            RibbleError::Core("Failed to open project directory.".to_string()),
        )?;

        let data_directory = proj_dirs.data_dir().to_path_buf();
        if !data_directory.exists() {
            std::fs::create_dir_all(data_directory.as_path())?;
        }

        debug_assert!(
            data_directory.is_absolute(),
            "Data dir path not canonicalized."
        );
        debug_assert!(data_directory.is_dir(), "Data dir not a directory.");

        // Set up the logger - duplicate to stderr in debug mode only to reduce IO.
        let mut logger = Logger::try_with_str("info")?
            .log_to_file(
                FileSpec::default()
                    .directory(data_directory.as_path())
                    .basename(Self::LOG_FILE_NAME),
            )
            .write_mode(WriteMode::BufferAndFlush)
            .rotate(
                Criterion::Age(Age::Day),
                Naming::Timestamps,
                Cleanup::KeepLogFiles(Self::MAX_LOG_FILES),
            );
        logger = if cfg!(debug_assertions) {
            logger.duplicate_to_stderr(Duplicate::All)
        } else {
            logger
        };
        // This needs to be kept alive until the app goes out of scope (consume on run).
        let logger_handle = logger.start()?;

        // Set up the crash handler
        let crash_handler = set_up_desktop_crash_handler()?;

        // Load the version & handle updates
        // The path gets canonicalized (and allocated) in the method, so only send the data directory here.
        let serialized_version = Self::deserialize_version(data_directory.as_path());

        // NOTE: at the moment there is no implementation for checking version updates.
        // It is undecided as of the present as to how this app will be distributed.
        let migration_version = MIGRATION_VERSION.get_or_init(|| {
            let min_ribble_version = RibbleVersion::new().set_major(0).set_minor(1).set_minor(2);
            RibbleVersion::default().set_min_compatible(min_ribble_version)
        });

        let version = if migration_version.compatible(serialized_version) {
            serialized_version
        } else {
            if let Err(e) = clear_old_ribble_state(data_directory.as_path()) {
                log::warn!(
                    "Error with clearing old data file: {e}\nError source:{:#?}",
                    e.source()
                );
            }

            let old_model_path = data_directory.join(OLD_MODEL_STUB);
            if let Err(e) = migrate_model_filenames(old_model_path.as_path()) {
                log::warn!(
                    "Error with renaming old model files: {e}\nError source: {:#?}",
                    e.source()
                );
            }
            *migration_version
        };

        // Construct the app
        // Give a copy of the data dir to the eframe window (for other persistence)
        // TODO -> since serialization is handled internally on drop, it mightn't be necessary to set the persistence path.
        // Look into this.
        let window_options = build_window(data_directory.clone());

        let app_path = data_directory.clone();
        let app: AppCreator<'_> = Box::new(move |cc| {
            let ribble_app = Ribble::new(version, app_path.as_path(), cc)?;
            Ok(Box::new(ribble_app))
        });

        // Return the runner
        Ok(RibbleRunner {
            version,
            data_directory,
            app: Some(app),
            window_options: Some(window_options),
            _logger: Some(logger_handle),
            _crash_handler: crash_handler,
        })
    }

    // NOTE: Calling run will consume the runner -> the version will get serialized on drop.
    pub fn run(mut self) -> Result<(), RibbleError> {
        log::info!("Starting Ribble.");
        let window_options = self
            .window_options
            .take()
            .ok_or(RibbleError::Core("Window not initialized.".to_string()))?;

        let app = self
            .app
            .take()
            .ok_or(RibbleError::Core("App not initialized.".to_string()))?;

        run_native(APP_ID, window_options, app)
            .map_err(|err| RibbleError::Eframe(err.to_string()))?;
        log::info!("Ribble window terminated.");
        Ok(())
        // -- Expect the app to be dropped here; if not, things might get a bit crusty with logging.
    }

    fn serialize_version(&self) {
        let canonicalized = self.data_directory.join(Self::VERSION_FILE_NAME);
        match File::create(canonicalized.as_path()) {
            Ok(version_file) => {
                let writer = BufWriter::new(version_file);
                match ron::Options::default().to_io_writer_pretty(
                    writer,
                    &self.version,
                    PrettyConfig::default(),
                ) {
                    Ok(_) => {
                        log::info!("Version file saved to: {:#?}", canonicalized.display());
                    }
                    Err(e) => {
                        log::warn!("Failed to serialize version: {e}");
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to open version file: {e}");
            }
        }
    }

    // NOTE: at this time, this constructs with default parameters as a mechanism
    // to know whether housekeeping has been performed and the app has been migrated
    // to at least the rewrite.

    fn deserialize_version(data_directory: &Path) -> RibbleVersion {
        let canonicalized = data_directory.join(Self::VERSION_FILE_NAME);
        match File::open(canonicalized.as_path()) {
            Ok(version_file) => {
                let reader = BufReader::new(version_file);
                ron::de::from_reader(reader).unwrap_or_else(|e| {
                    log::warn!("Failed to deserialize version: {e}");
                    RibbleVersion::new()
                })
            }
            Err(e) => {
                log::warn!("Failed to open version file: {e}");
                RibbleVersion::new()
            }
        }
    }
}

impl Drop for RibbleRunner<'_> {
    fn drop(&mut self) {
        log::info!("Dropping ribble runner, serializing version.");
        self.serialize_version();
        log::info!("Final serialize completed.");
    }
}

#[inline]
fn build_window(persistence_path: PathBuf) -> NativeOptions {
    NativeOptions {
        viewport: build_viewport(),
        // TODO: this seems to be causing an error.
        // For some reason eframe::native::file_storage is trying to create a "ribble" file instead of treating this as a directory.
        persistence_path: Some(persistence_path),
        persist_window: true,
        ..Default::default()
    }
}

#[inline]
#[cfg(not(target_os = "macos"))]
fn build_viewport() -> ViewportBuilder {
    ViewportBuilder::default()
        .with_app_id(APP_ID)
        .with_title(APP_ID)
        .with_resizable(true)
        .with_icon(load_icon())
        // NOTE: if maximizing is too annoying, go back to a default size.
        // TODO: this is not being respected atm -> look into it.
        .with_maximized(true)
}

// TODO: MacOs may require more "Apple-like" configurations.
#[inline]
#[cfg(target_os = "macos")]
fn build_viewport() -> ViewportBuilder {
    ViewportBuilder::default()
        .with_app_id(APP_ID)
        .with_title(APP_ID)
        .with_resizable(true)
        .with_titlebar_shown(false)
        .with_icon(load_icon())
        .with_maximized(true)
}
fn load_icon() -> Arc<IconData> {
    image::load_from_memory(ICON_BYTES)
        .ok()
        .map(|image| {
            let (i_width, i_height) = image.dimensions();
            Arc::new(IconData {
                rgba: image.to_rgba8().to_vec(),
                width: i_width,
                height: i_height,
            })
            // This is explicitly the OS default instead of "egui's" default.
        })
        .unwrap_or(Arc::new(IconData::default()))
}
