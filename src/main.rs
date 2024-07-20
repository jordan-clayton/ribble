use directories::ProjectDirs;

mod preferences;
mod serialize;
mod constants;

fn main() {
    println!("Hello, world!");
    let proj_dir = ProjectDirs::from("com", "Jordan", "WhisperGUI").expect("No home folder");
    // let mut wg_configs: configs::Configs = serialize::load_configs(&proj_dir);
    // TODO: Migrate this to GUI project.
    let mut wg_prefs: preferences::GUIPreferences = serialize::load_prefs(&proj_dir);
}
