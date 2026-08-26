# Ribble

Realtime (streaming) and offline (file) transcription using OpenAI's Whisper ATS.
This app has undergone a ***significant*** rewrite, but is currently functional and (arguably) quite useful.
Expect there to be bugs and surprises until the next release.

***This software is not yet stable***, keep that in mind. If you encounter issues,
see [troubleshooting](#troubleshooting) for advice on how to deal with known problems. Otherwise, please feel free to
file an issue and I will look into it when I can.

## Migration

If you happened to use the very first Ribble release, there are mechanisms in place to perform a best-effort full
migration over to the newest version. This will run on app launch and may cause a bit of delay. If you encounter any
problems, locate the Ribble folder in your user data folder (APPDATA, .local, ApplicationSupport, etc.);
back up the models you wish to continue using, and delete the old folder.

## How to use

Full documentation/instructions are TBD. There are tooltips in Ribble that show on hover which should hopefully
explain what things do, but for a quick run-down of things which may not be immediately clear:

- Additional panes/tabs and user settings can be accessed via the hamburger menu in the top right corner
- When the visualizer pane is in focus, use the arrow keys: left and right to swap the visualization algorithm
    - This can also be changed via the context menu (right-click)
- For panes/views which can be closed, use the context menu (right-click) and select close
- You may need to grant microphone permissions to use the application
- At the moment, only one copy of each pane/tab is expected to exist in the main window at any time
    - If you try to open it again, the pane will be brought into focus and highlighted
- All panes are dock-able and drag-able
    - Just click and drag panes and drop them where you want them to go
    - If at some point the window crashes/disappears, just reset the layout from the hamburger menu

- If you encounter major issues, see: [troubleshooting](#troubleshooting).
- There is an error console in Ribble which should provide some more information in the case of a major error.

## Models

Ribble is compatible with ggml- whisper models which are
available [here](https://huggingface.co/ggerganov/whisper.cpp/tree/main). They are no longer distributed within the
application by default, but they are licensed under MIT and available to download. You will require at least one model
to be able to run transcription. You can set your desired model in the transcriber configurations.

If you are building from source, it is possible to embed two small quantized models with the pack-in-models feature
flag. You will need to have git lfs to pull the models in before embedding the models within the application.
These have very low accuracy, but may prove useful for streaming/testing on lower-end hardware.

There are buttons in the transcriber control pane for:

- Opening the model folder for manual copy/drag-and-drop
- Copying model files over to Ribble's application folder
- Downloading models via url or automatically pulling from hugging-face

It should be a fairly straightforward process to install models in the app. Full documentation/instructions are planned
TBD.
To use Ribble's URL downloader:

- Go to the model file page, (e.g. [here](https://huggingface.co/ggerganov/whisper.cpp/blob/main/ggml-medium-q8_0.bin))
- Click on "Copy download link"
- Paste it in the url field in Ribble
- Click download
- When the model is fully (and successfully) downloaded, it should appear in the model selection drop-down menu in the
  configurations.

### Recommendations

The accuracy of both realtime and offline transcription is highly dependent on the size of the model you use.
[ggml-medium](https://huggingface.co/ggerganov/whisper.cpp/blob/main/ggml-medium.bin) seems to work well in realtime
with modern hardware. 
[ggml-medium-q8_0](https://huggingface.co/ggerganov/whisper.cpp/blob/main/ggml-medium-q8_0.bin) will be faster but can be slightly less accurate (or sometimes more; it all depends and your mileage may vary).
Older hardware may require smaller models to achieve acceptable realtime performance.
Alternatively, consider changing the buffering strategy in the realtime configurations to buffered to trade
latency for accuracy. You also need to ensure you have enough memory to load the model that you use,
otherwise the stream will not run--if the transcription error is recoverable (in most cases, it should be),
detailed information will be in the console pane.

#### Quantized models

The q5_x and q8_x quantized models tend to be very performant and significantly save on model size.
I would recommend looking into them if your use-case tends toward realtime streaming. Accuracy may be lower than the
non-quantized models, but can sometimes perform better due to how the streaming implementation works.

## Building
### Additional Dependencies
- A C++ compiler
- [Rust](https://rust-lang.org)
- [Cmake >= 3.24](cmake.org)
- SDL 2 ( or SDL 3 with a bridge like sdl2-compat ).
  - This is available for MacOS via homebrew.
  - SDL2 Releases can also be obtained [here](https://github.com/libsdl-org/SDL/releases/tag/release-2.32.2).
    - SDL2 is required at runtime for the audio backend.
    - If you prefer to link statically, you must have the following:
      - A version of CMake that can build a CMAKE 3.1 project.
      - a C compiler.

- One of the following three should be installed to compile with a GPU backend for realtime streaming:
  - (optional) CUDA 12+ ( Windows/Linux + NVIDIA only ).
  - (optional) [ROCm/HipBLAS](https://github.com/ROCm/rocm-libraries) ( AMD + Linux only ).
  - (optional) [Vulkan](https://vulkan.lunarg.com) ( Linux/ Windows ).
  - MacOS metal is included in MacOS; you do not need to download an SDK.

- Cargo will pull in all rust-related dependencies; many have been selected because they are purely Rust but not all. If you encounter additional linking errors, you may have to seek out COME BACK

If you want to use the packed quantized models, you will require [git-lfs](https://git-lfs.com/)
You will require to clone this repository recursively to pull in all submodules
```git clone --recursive ...```
Pulling changes will also require you to pull in changes to submodules and, if embedding models, perform a git lfs pull
```git pull --recurse-submodules```
```git lfs pull```

Build with cargo with the following:
```cargo build --release```

It is recommended to build with one of the following features to enable GPU acceleration:
```cargo build --release --features <gpu backend>```

To embed the pack-in models into the application:
``cargo build --release --features "pack-in-models ...<other features>" ``

The application binary will be in target/release.

## Features 
### GPU Acceleration
- ```cuda``` for cuda support (Linux/Windows). Requires a Cuda Toolkit >= 11.8.
- ```metal``` for metal support (macOS only)
- ```coreml``` for coreml support (macOS only; implicitly enables metal).
- ```vulkan``` for Vulkan support. Requires the Vulkan Sdk.
- ```hipblas``` for ROCm/hipBlas support (Linux). This has not been tested.
- ```log-whisper``` enables logging support when running whisper.

OpenBlas is available (Windows-only) for CPU acceleration but has not yet been exposed. File an issue if you require
this feature either here or in [ribble-whisper](https://github.com/jordan-clayton/ribble-whisper).
### Additional features
- ```sdl2-static``` will pull in SDL2, build it, and statically link with it.
  - Only use this if you do not already have SDL2 installed
  - It is highly discouraged for Linux users to link statically with SDL2; use the one provided by your distribution for best results and to avoid quirks.
  - Planned release builds for Windows and MacOS will include a copy of SDL2, so this should not be of concern.
  - Planned release builds for Linux are TBD, but will not include SDL2; obtain this from your package manager.

- ```pack-in-models``` embeds two small quantized models in the application, see src/models.
  - Avoid this feature whenever possible.
  - Prefer downloading a larger ggml model for better accuracy and to reduce the executable size.

## CUDA

- It is possible to set CMAKE/Whisper environment variables to build for a target architecture
- The instructions for this are TBD, but will be added here at some point
- Building with the default configurations will compile sm_86 and sm_89 and compute for most common architectures (RTX
  30XX-RTX40XX)
- If you have an older card, you may run into jitter/slowdown on first run because of JIT
- Subsequent runs after JIT compilation will be significantly faster due to caching
- Of the testing done thus far, CUDA far exceeds expectations
- It is possible to build from source and compile for your own hardware's architecture by way of CMAKE/GGML
  environment variables
    - GGML_NATIVE = "ON" will build for your specific hardware; this will be the easiest to build.
    - ... Additional information here is still TBD.
    - Any environment variables prefaced with GGML_* or CMAKE_* should be caught and passed in.
    - See [here](https://github.com/ggml-org/whisper.cpp/blob/master/ggml/src/ggml-cuda/CMakeLists.txt) for further details. 
- It is infeasible for me to provide CUDA builds for all supported GPUs on Linux and Windows.
 - If you wish to use the CUDA backend ( it is encouraged for NVIDIA users with RTX 2XXX or newer ), you will have to build from source.
 - The Vulkan performance is excellent and is the planned backend for an eventual Windows/Linux release.


## CoreML

- I don't have the hardware to test this, nor am I familiar enough with CoreML to provide great advice here
- There are clearer instructions [here](https://github.com/ggml-org/whisper.cpp?tab=readme-ov-file#core-ml-support)
- There are also pre-prepared CoreML models from [here](https://huggingface.co/ggerganov/whisper.cpp/tree/main): these
  end in mlmodelc.zip
- To use a CoreML model:
    - Unzip the archive and place the folder within Ribble's model folder.
    - Upon loading a CoreML model for the first time, expect some significant stutter and lag until things are prepared
      for
      the ANE
    - Subsequent runs should be much quicker; try running a test on a short recording
    - At this time I'm not much help to resolve any CoreML-specific issues, but hopefully that'll change
    - If CoreML fails for reasons I don't know about--the app is expected to fall back to Metal and still run on the
      GPU.

## Vulkan

- It is more than sufficient for realtime applications and should be expected to perform well in most cases
- There are plans to prioritize Vulkan by default in a main release stream

## HipBLAS/ROCm (Linux-only)

- I do not have AMD hardware and I have no idea what to expect here

## Troubleshooting

### CUDA fails to build when using ccache/sccache:
Yes and I wish I could tell you why it breaks. You can generally bet that whenever
a new version of CUDA gets released, it will also *completely fail* to work with caching, so just turn it off for CUDA builds.

Set an environment variable: GGML_CCACHE = "OFF" for CUDA builds and it should work fine.


### Realtime is running slowly/latency is poor:
See the following:

#### Realtime runs slowly on hybrid gpus:
Run ribble on your discrete card (e.g. using prime-run). You're likely running on the CPU without realizing and
this is generally infeasible for realtime ATS.

#### The CUDA backend is really slow:
This is most likely the JIT-compilation. Make sure one: your hardware is supported, two: you built for the right architecture. 
  
If you're using PTX (virtual architecture) intentionally (e.g. for smaller executable size/multiple architectures) the best way to mitigate the issue is to "warm up" Ribble by running transcription on a small amount of audio with the model you require. This will prompt the JIT to lower PTX down to your particular GPU. Run a short realtime stream and speak into the mic until you see text appear in the window. Subsequent transcriptions should be extremely fast.

#### Laggy/Slow start:
Depending on your VAD/GPU backend configurations, there can be a small amount of startup latency.
( I have personally experienced this with the Vulkan backend--initial hiccups tend to disappear after the first 
  round of inference ).
As soon as ribble shows "\[START SPEAKING\]", the transcription is recording your voice, so anything that
gets missed can be re-transcribed offline after the session.
As soon as you see text output to the screen, there is a transcription loop running and so you can proceed as normal.
Just continue speaking and the loop should catch up and clean itself up while it's transcribing.

The Silero backend is known to be picky about poor signal/quiet input and may produce false negatives.
It also tends to need to be warmed up to properly detect voice. These may be contributing to the issue;
try Earshot (or WebRtc) instead, and look into increasing the audio gain/input volume.

You may also just have to tweak the exposed configurations until you get something that works well most of the time. 
I don't know your hardware and I don't know your audio configuration. I have done what I can to try and alleviate the complexity,
but Ribble solves a very complex problem.

I realize this is not the most ideal or seamless experience. In the interest of abstracting away highly technical
configurations, I have decided to limit what information gets displayed to the UI--but this can and should be improved.
Expect incremental fixes to the implementation and the UI; I do not have a lot of time to put into this project.


### Duplicated words when streaming realtime:

These are expected due to the way the implementation was handled, but expect most of these duplications to resolve as
the stream continues. There are measures in place to clean up the transcription while it runs.
It can sometimes still fail if the audio is unclear, (e.g. has / as), or if the model is small. Ribble attempts to
record the stream while it's transcribing--provided there are no issues with the recording, it will be available
offline transcribing after the stream has finished.

### The transcript is inaccurate:

Use a larger model if you can support it. Some larger models, (e.g. v1, v2, v3) can sometimes be less accurate. You may
see better results with medium or large-v3-turbo. I would recommend using the medium model if you can.

### Whisper is hallucinating:

This is a known issue with whisper. It tends to struggle with quiet audio signals/silence, so you may see some
hallucinations when transcribing (both in realtime and offline). When streaming realtime, this **should** get
corrected as the transcription goes on, but errors are known to get through occasionally. For either realtime or offline, try
applying audio gain, and when transcribing files, try toggling the File VAD option to prune out silent frames.

### I'm speaking, but there's nothing appearing on the screen:

Try turning up your microphone's volume, or turning up the audio gain (or both). It's most likely that your signal is
too quiet and is being detected as "silence" or "no voice activity." You can also try tweaking the VAD settings. Silero
v6  struggles with quiet audio and can be too strict. Try another VAD backend (Earshot, WebRtc) or set the strictness to flexible.

### All the panes and tabs are missing:

I have tried my best to try and mitigate this and prevent it from happening. There may still be cases where the
layout becomes incoherent, but the issue is not expected to persist across sessions. Try restarting the
application to recover the last layout; it will fall back to defaults if it cannot be recovered.
Otherwise, open the hamburger menu and select "Reset layout" to restore manually.

### (Bad Magic) error:

The model is not correct/malformed/corrupted/not up-to-date. If this happens why you try to use an embedded model,
rebuild the project, but pull in the models using git lfs first.

### Streaming is too slow:

Try using a smaller model--or look at using one of the quantized models. Also: set hardware acceleration to "on" to
avoid running transcription on the CPU. Try setting the buffering to continuous if you have moderately decent hardware and
are using a GPU backend.

If this is still infeasible, you may need to just record audio and transcribe it offline. I have done what I can thus
far to get the application running on extremely modest, decade-old hardware (~2013 intel iGPU)--realtime should still be feasible with very small models or, if you have more (GPU) memory, a slightly larger one.


### The application crashed:

If you encounter any random crashes, please let me know by filing an issue. This software is in its infancy, so there
are bound to be uncaught bugs and errors. If you could provide me with as much information as you can, your operating
system,
which version of Ribble you're running, and--if you're comfortable searching in the application folder--the relevant
information from the most current crash-log. Your help would be most appreciated!

If Ribble crashes due to memory errors/GPU errors, expect an OS dialog to appear with some more information about what
caused the crash. File an issue and I will look into it ASAP! Otherwise, everything else should be logged.

## TODO:

Proper application documentation and instructions.
Release 0.1.4

## Licensing:

All licensing information for dependencies and related code can be found under THIRD_PARTY_LICENSES.md and
LICENSE-WHISPERCPP.

SDL2 is licensed by zlib.
