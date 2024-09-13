# Ribble 
Realtime and static (offline) transcription using OpenAI's Whisper ATS.
App is currently functional, build with cargo with the following:
```cargo build --release```
or run the appropriate buildscript.

NOTE: the buildscripts also require cargo packager as a build dependency.

The buildscripts will output the installer in ```./dist/```


Use the --features flag to include support for the following features.

## Features

- ```cuda``` for cuda support (Linux/Windows). Requires the cuda toolkit to be installed to compile.
- ```metal``` for metal support (default for macOS when using buildscript).
- ```rocm``` for RocM support (Linux). This has not been tested.
- features in [whisper-rs](https://github.com/tazz4843/whisper-rs?tab=readme-ov-file#feature-flags)

Your mileage may vary with the untested feature flags. Testing is not yet complete.

## Linux
The .deb cuda release requires major version 12: [cuda-toolkit-12-4](https://developer.nvidia.com/cuda-12-4-0-download-archive).
It has been confirmed to work with toolkits 12.4 and 12.5.

The Appimage should work out of the box, provided you have nvidia driver version >= 550.28

## TODO:
Remaining documentation.
Release builds.
