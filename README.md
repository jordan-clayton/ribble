# Ribble 
Realtime and static (offline) transcription using OpenAI's Whisper ATS.
App is currently functional, build with cargo with the following:
```cargo build --release```
or run the appropriate buildscript.

Use the --features flag to include support for the following features.

## Features

- ```cuda``` for cuda support (Linux/Windows). Requires the cuda toolkit to be installed to compile.
- ```metal``` for metal support (default for macOS when using buildscript).
- ```rocm``` for RocM support (Linux). This has not been tested.
- features in [whisper-rs](https://github.com/tazz4843/whisper-rs?tab=readme-ov-file#feature-flags)

Your mileage may vary with the untested feature flags. Testing is not yet complete.

## TODO:
Remaining documentation.
Release builds.
