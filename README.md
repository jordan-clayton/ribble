# Ribble

Real-time (streaming) and offline (file) transcription using OpenAI's Whisper ATS.
This app has undergone a ***significant*** rewrite, but is currently functional and (arguably) quite useful.
Expect there to be bugs and surprises until the next release.

***This software is not yet stable***

## Migration

If you happened to use the very first Ribble release, there are mechanisms in place to perform a best-effort full
migration
over the newest version. This will run on app launch and may cause a bit of delay. If you encounter any problems,
locate the Ribble folder in your user data folder (APPDATA, .local, ApplicationSupport, etc.); back up the models you
wish to continue using, and delete the folder.

## Models

Ribble has a download API to retrieve models from huggingface, or via a download URL.
Very little work has gone into url sanitation and security, so practice caution before downloading.
(This will hopefully be changed in the future). Ribble now embeds two small quantized models into the application, so
you can start transcribing as soon as the application loads.

### Quantized models

These tend to be highly performant and significantly save on model size. I would recommend looking into them
if your use case tends toward streaming.

### Accuracy

Larger models (unsurprisingly) tend to be more accurate, but not always. Be mindful of your GPU specs and VRAM or you
may encounter memory errors.

- Medium (non-quantized) works well in real-time if your hardware can support it.

Build with cargo with the following:
```cargo build --release```

It is recommended to build with one of the following features to enable GPU acceleration.
```cargo build --release --features <gpu backend>```

Ignore the buildscripts for distribution/packaging. Things need to be re-addressed and your experience will be smoother
with cargo.

## Features (GPU Acceleration)

- ```cuda``` for cuda support (Linux/Windows). Requires a Cuda Toolkit >= 11.8.
- ```metal``` for metal support (macOS only)
- ```coreml``` for coreml support (macOS only; implicitly enables metal).
- ```vulkan``` for Vulkan support. Requires the Vulkan Sdk.
- ```hipblas``` for ROCm/hipBlas support (Linux). This has not been tested.
- More features may be added at a later date.

## CUDA

- It is possible to set CMAKE/Whisper environment variables to build for a target architecture
- The instructions for this are TBD, but will be added here at some point
- Building with the default configurations will compile sm_86 and sm_89 and compute for most common architectures (RTX
  30XX-RTX40XX)
- If you have an older card, you may run into jitter/slowdown on first run because of JIT; Subsequent runs will be
  significantly faster
- Of the (only) testing done thus far, CUDA far exceeds expectations

## CoreML

- TBD: I don't have the hardware to test/make any claims about how to set things up.
- There are instructions [here](https://github.com/ggml-org/whisper.cpp?tab=readme-ov-file#core-ml-support)
- There are also precompiled CoreML models available, your mileage may vary.
- At this time I'm not much help to resolve any issues, but hopefully that'll change.

## Vulkan

- I have yet to test a Vulkan build. Again, your mileage may vary.
- There are plans to support Vulkan in a release stream

## TODO:

Remaining documentation.
Model info/recommendations.
Instructions.
Release stream.
Fix licensing.

## Licensing:

This project was initially written with the intent to be released under MIT only. As of the app rewrite, this may no
longer be the case. See: [ribble_whisper](https://github.com/jordan-clayton/ribble-whisper). This is in progress.