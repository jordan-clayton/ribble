// A collection of tools to make migrating between versions of Ribble a little bit easier.
use crate::utils::errors::RibbleError;
use crate::utils::include;
use ribble_whisper::whisper::model::DefaultModelType;
use std::error::Error;
use std::fmt::Display;
use std::num::ParseIntError;
use std::path::{Path, PathBuf};
use strum::IntoEnumIterator;

#[derive(Debug)]
pub(crate) enum VersionError {
    ParseIntError(ParseIntError),
    InvalidFormat,
}

impl From<ParseIntError> for VersionError {
    fn from(error: ParseIntError) -> Self {
        VersionError::ParseIntError(error)
    }
}

impl Display for VersionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionError::ParseIntError(parse_error) => {
                write!(f, "SemVer Number Parse Error: {parse_error}.")
            }
            VersionError::InvalidFormat => write!(f, "Invalid SemVer string"),
        }
    }
}

impl Error for VersionError {}

pub(crate) trait Version: Display + Default {
    fn major(&self) -> usize;
    fn minor(&self) -> usize;
    fn patch(&self) -> usize;
    fn from_cfg() -> Self;
    fn compatible(&self, other: Self) -> bool;
    fn increment_major(self) -> Self;
    fn increment_minor(self) -> Self;
    fn increment_patch(self) -> Self;
    fn from_semver_string(semver: &str) -> Result<Self, VersionError>;
    fn semver_string(&self) -> String;
    fn into_semver_string(self) -> String;
}
#[derive(Debug, Clone, Copy, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct RibbleVersion {
    major: usize,
    minor: usize,
    patch: usize,
    // For backward compatibility.
    min_compatible: (usize, usize, usize),
}

impl RibbleVersion {
    pub(crate) fn new() -> Self {
        Self {
            major: 0,
            minor: 0,
            patch: 0,
            min_compatible: (0, 0, 0),
        }
    }

    pub(crate) fn set_major(mut self, major: usize) -> Self {
        self.major = major;
        self
    }
    pub(crate) fn set_minor(mut self, minor: usize) -> Self {
        self.minor = minor;
        self
    }
    pub(crate) fn set_patch(mut self, patch: usize) -> Self {
        self.patch = patch;
        self
    }
    pub(crate) fn set_min_compatibile(mut self, min_compatible: Self) -> Self {
        self.min_compatible = min_compatible.into();
        self
    }
}

impl From<(usize, usize, usize)> for RibbleVersion {
    fn from(value: (usize, usize, usize)) -> Self {
        Self::new()
            .set_major(value.0)
            .set_minor(value.1)
            .set_patch(value.2)
    }
}

impl From<RibbleVersion> for (usize, usize, usize) {
    fn from(value: RibbleVersion) -> Self {
        (value.major, value.minor, value.patch)
    }
}

impl Version for RibbleVersion {
    fn major(&self) -> usize {
        self.major
    }

    fn minor(&self) -> usize {
        self.minor
    }

    fn patch(&self) -> usize {
        self.patch
    }

    fn from_cfg() -> Self {
        const VER: &'static str = env!("CARGO_PKG_VERSION");
        Self::from_semver_string(VER).expect("From semver string expects Cargo semver format.")
    }

    fn compatible(&self, other: Self) -> bool {
        let other_comp: (usize, usize, usize) = other.into();
        other_comp > self.min_compatible
    }

    fn increment_major(mut self) -> Self {
        self.major += 1;
        self.minor = 0;
        self.patch = 0;
        self
    }

    fn increment_minor(mut self) -> Self {
        self.minor += 1;
        self.patch = 0;
        self
    }

    fn increment_patch(mut self) -> Self {
        self.patch += 1;
        self
    }

    fn from_semver_string(semver: &str) -> Result<Self, VersionError> {
        let parts = semver.split(".").collect::<Vec<_>>();
        if parts.len() != 3 {
            Err(VersionError::InvalidFormat)
        } else {
            let [major_str, minor_str, patch_str] = parts[..] else {
                unreachable!("Format must be exactly 3 strs to unpack.");
            };

            Ok(Self::new()
                .set_major(major_str.parse::<usize>()?)
                .set_minor(minor_str.parse::<usize>()?)
                .set_patch(patch_str.parse::<usize>()?))
        }
    }

    fn semver_string(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    fn into_semver_string(self) -> String {
        self.semver_string()
    }
}

impl Display for RibbleVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.semver_string())
    }
}

impl Default for RibbleVersion {
    fn default() -> Self {
        Self::from_cfg()
    }
}

// NOTE: this might go unused, if so, remove
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct RibbleMigration {
    version: RibbleVersion,
    current_model_directory: PathBuf,
    model_includes_copied: bool,
}

impl RibbleMigration {
    pub(crate) fn new(version: RibbleVersion, current_model_directory: PathBuf) -> Self {
        Self {
            version,
            current_model_directory,
            model_includes_copied: false,
        }
    }

    pub(crate) fn with_version(mut self, new_version: RibbleVersion) -> Self {
        self.version = new_version;
        self
    }

    pub(crate) fn with_model_directory(mut self, new_model_directory: &Path) -> Self {
        self.current_model_directory = new_model_directory.to_path_buf();
        self.model_includes_copied = include::confirm_models_copied(&self.current_model_directory);
        self
    }

    pub(crate) fn copy_model_includes(&mut self) -> Result<(), RibbleError> {
        let res = include::copy_model_includes(self.current_model_directory.as_path());
        self.model_includes_copied = res.is_ok();
        res
    }

    pub(crate) fn version(&self) -> RibbleVersion {
        self.version
    }

    pub(crate) fn model_includes_copied(&self) -> bool {
        self.model_includes_copied
    }
}

// Since Ribble v (0.0.1) used different file-name conventions, this needs to be called at least
// once on the first launch of the new version to change model file names over to the new ones.
// This is a best-effort sort of deal; if folks have migrated their own models/subbed in models,
// this will not preserve the integrity.
//
// NOTE: remember to make a note of this in the instructions.
pub(crate) fn migrate_model_filenames(model_directory: &Path) -> Result<(), std::io::Error> {
    for default_model_type in DefaultModelType::iter() {
        let test_path = model_directory.join(default_model_type.old_file_name());
        if test_path.is_file() {
            let new_path = model_directory.join(default_model_type.to_file_name());
            std::fs::rename(test_path, new_path)?;
        }
    }
    Ok(())
}

const OLD_STATE_FILE_NAME: &'static str = "data.ron";

// If it becomes important to know whether this actually removed the file, change the
// return type to something that can communicate that.
pub(crate) fn clear_old_ribble_state(data_directory: &Path) -> Result<(), std::io::Error> {
    match std::fs::remove_file(data_directory.join(OLD_STATE_FILE_NAME)) {
        Ok(_) => Ok(()),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}
