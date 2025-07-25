use crate::ui::app::Ribble;
use crate::utils::crash_handler::set_up_desktop_crash_handler;
use crate::utils::errors::RibbleError;
use crate::utils::migration::RibbleVersion;
use crash_handler::CrashHandler;
use directories::ProjectDirs;
use eframe::{run_native, AppCreator, NativeOptions};
use egui::{IconData, ViewportBuilder};
use flexi_logger::{Age, Cleanup, Criterion, Duplicate, FileSpec, Logger, LoggerHandle, Naming, WriteMode};
use image::GenericImageView;
use ron::ser::PrettyConfig;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const APP_ID: &str = "Ribble";
pub const QUALIFIER: &str = "com";
pub const ORGANIZATION: &str = "Jordan";

#[cfg(not(target_os = "macos"))]
pub const ICON_PATH: &str = "assets/whisper_app_icon_128x128@1x.png";
#[cfg(target_os = "macos")]
pub const ICON_PATH: &str = "assets/whisper_app_icon_1024x1024@1x.png";

pub(crate) struct RibbleRunner {
    version: RibbleVersion,
    // NOTE: this -could- have just a path reference, perhaps that might be better.
    data_directory: PathBuf,
    app: AppCreator<'_>,
    window_options: NativeOptions,
    logger: Option<LoggerHandle>,
    // TODO: possibly make this an option?
    // Not quite sure; it's most likely a good idea to have one though in case Whisper.cpp segfaults.
    crash_handler: CrashHandler,
}

impl RibbleRunner {
    const VERSION_FILE_NAME: &'static str = "version.ron";
    // TODO: if this constant needs to change, change it
    // Right now it's good for a "week's worth".
    const MAX_LOG_FILES: usize = 7;
    const LOG_FILE_NAME: &'static str = "ribble_log";

    pub(crate) fn new() -> Result<Self, RibbleError> {
        // Set up the project directory
        let proj_dirs = ProjectDirs::from(
            QUALIFIER,
            ORGANIZATION,
            APP_ID,
        ).ok_or(RibbleError::Core("Failed to open project directory.".to_string()))?;

        let data_directory = proj_dirs.data_dir().to_path_buf();
        if !data_directory.exists() {
            std::fs::create_dir_all(data_directory.as_path())?;
        }

        debug_assert!(data_directory.is_absolute(), "Data dir path not canonicalized.");
        debug_assert!(data_directory.is_dir(), "Data dir not a directory.");

        // Set up the logger - duplicate to stderr in debug mode only to reduce IO.
        let mut logger = Logger::try_with_str("info")?
            .log_to_file(FileSpec::default()
                .directory(data_directory.as_path())
                .basename(Self::LOG_FILE_NAME)
            )
            .write_mode(WriteMode::BufferAndFlush)
            .rotate(
                Criterion::Age(Age::Day),
                Naming::Timestamps,
                Cleanup::KeepLogFiles(Self::MAX_LOG_FILES),
            );
        logger = if cfg!(debug_assertions) {
            logger
                .duplicate_to_stderr(Duplicate::All)
        } else {
            logger
        };
        // This needs to be kept alive until the app goes out of scope (consume on run).
        let logger_handle = logger.start()?;

        // Set up the crash handler
        let crash_handler = set_up_desktop_crash_handler()?;

        // Load the version & handle updates
        // The path gets canonicalized (and allocated) in the method, so only send the data directory here.
        let version = Self::deserialize_version(data_directory.as_path());
        // TODO: check for "needs update" or similar and do something about it.
        // Perhaps just open a link to the new releases page.
        // Handle migration if not already done -> this needs some tlc,
        // especially with the model bank.

        // TODO: add a field for "has migrated" -> double check the versioning struct.

        // Construct the app
        // Give a copy of the data dir to the eframe window (for other persistence)
        // TODO -> since serialization is handled internally on drop, it mightn't be necessary to set the persistence path.
        // Look into this.
        let window_options = build_window(data_directory.clone());

        let app = Box::new(|cc| {
            let ribble_app = Ribble::new(data_directory.as_path(), cc)?;
            Ok(Box::new(ribble_app))
        });

        // Return the runner
        Ok(RibbleRunner {
            version,
            data_directory,
            app,
            window_options,
            logger: Some(logger_handle),
            crash_handler,
        })
    }

    // NOTE: Calling run will consume the runner.
    pub(crate) fn run(self) -> Result<(), RibbleError> {
        log::info!("Starting Ribble.");
        run_native(APP_ID, self.window_options, self.app)?;
        log::info!("Ribble window terminated.");
        Ok(())
        // -- Expect the app to be dropped here; if not, things might get a bit crusty with logging.
    }

    fn serialize_version(&self) {
        let canonicalized = self.data_directory.join(Self::VERSION_FILE_NAME);
        match File::create(canonicalized.as_path()) {
            Ok(version_file) => {
                let writer = BufWriter::new(version_file);
                match ron::Options::default().to_io_writer_pretty(writer, &self.version, PrettyConfig::default()) {
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

    fn deserialize_version(data_directory: &Path) -> RibbleVersion {
        let canonicalized = data_directory.join(Self::VERSION_FILE_NAME);
        match File::open(canonicalized.as_path()) {
            Ok(version_file) => {
                let reader = BufReader::new(version_file);
                ron::de::from_reader(reader).unwrap_or_else(|e| {
                    log::warn!("Failed to deserialize version: {e}");
                    RibbleVersion::default()
                })
            }
            Err(e) => {
                log::warn!("Failed to open version file: {e}");
                RibbleVersion::default()
            }
        }
    }
}

impl Drop for RibbleRunner {
    fn drop(&mut self) {
        log::info!("Dropping ribble runner.");
        self.serialize_version();
    }
}


#[inline]
fn build_window(persistence_path: PathBuf) -> NativeOptions {
    NativeOptions {
        viewport: build_viewport(),
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
        .with_icon(load_icon("assets/whisper_app_icon_1024x1024@1x.png"))
        .with_maximized(true)
}
fn load_icon() -> Arc<IconData> {
    image::load_from_memory(include_bytes!(ICON_PATH))
        .ok().and_then(|image| {
        let (i_width, i_height) = image.dimensions();
        Some(Arc::new(
            IconData {
                rgba: image.to_rgba8().to_vec(),
                width: i_width,
                height: i_height,
            }
        ))
        // This is explicitly the OS default instead of "egui's" default.
    }).unwrap_or(Arc::new(IconData::default()))
}
