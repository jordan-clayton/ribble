use directories::ProjectDirs;

// This should probably be split into separate str qualifiers -> needs to be used for eframe.
// I DO NOT LIKE THE WAY THIS IS IMPLEMENTED.
fn proj_dir()-> Option<ProjectDirs> { ProjectDirs::from("com", "Jordan", "WhisperGUI")}
