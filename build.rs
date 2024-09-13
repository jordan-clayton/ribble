use directories::ProjectDirs;
use std::{fs, io, path::Path};

fn main() {
    const PROTOTYPE_APP_ID: &str = "WhisperGUI";
    const APP_ID: &str = "Ribble";
    const QUALIFIER: &str = "com";
    const ORGANIZATION: &str = "Jordan";

    // for SDL2
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");

    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");

    // NOTE: this is not wise practice and is only in place until all development machines have
    // migrated to the new app name.
    // TODO: Remove this once files have been migration
    // This does not need to be in release code.
    let old_proj_dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, PROTOTYPE_APP_ID);

    // Abort proj folder migration if no access to project folders.
    // The app will not run anyway if permissions aren't granted.
    if old_proj_dirs.is_none() {
        return;
    }

    let old_proj_dirs = old_proj_dirs.unwrap();

    let new_proj_dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, APP_ID);

    if new_proj_dirs.is_none() {
        return;
    }

    let new_proj_dir = new_proj_dirs.unwrap();

    let old_proj_data_dir = old_proj_dirs.data_dir();
    // Escape if no old data exists
    if !old_proj_data_dir.exists() {
        return;
    }

    let new_proj_data_dir = new_proj_dir.data_dir();

    // If the new folder already exists, data has been migrated.
    if new_proj_data_dir.exists() {
        return;
    }

    // Copy the data over.
    if let Ok(_) = copy_folder(old_proj_data_dir, new_proj_data_dir) {
        let _ = fs::remove_dir_all(old_proj_data_dir);
    };
}

fn copy_folder(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(dst.as_ref())?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            copy_folder(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}
