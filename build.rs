fn main(){
    // for SDL2
    #[cfg(target_os="macos")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");

    #[cfg(target_os="linux")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");

    // TODO: Once app name properly chosen, migrate the prototype folder to the
    // new one on build.
}
