use ribble_whisper::audio::audio_backend::AudioBackend;
use ribble_whisper::audio::audio_backend::CaptureSpec;
use ribble_whisper::audio::microphone::{MicCapture, Sdl2Capture};
use ribble_whisper::audio::recorder::{ArcChannelSink, SampleSink};
use ribble_whisper::utils::errors::RibbleWhisperError;
use ribble_whisper::utils::{get_channel, Sender};
use std::error::Error;
use std::sync::Arc;

pub enum AudioCaptureRequest {
    Open(
        CaptureSpec,
        ArcChannelSink<f32>,
        Sender<Result<SharedSdl2Capture<ArcChannelSink<f32>>, RibbleWhisperError>>,
    ),
    Close(usize),
}

pub struct AudioBackendProxy {
    request_sender: Sender<AudioCaptureRequest>,
}

impl AudioBackendProxy {
    pub fn new(request_sender: Sender<AudioCaptureRequest>) -> Self {
        Self { request_sender }
    }
}

// For all intents and purposes, concrete types are fine.
// Until that requirement changes, avoid generics.
impl AudioBackend<ArcChannelSink<f32>> for AudioBackendProxy {
    type Capture = SharedSdl2Capture<ArcChannelSink<f32>>;

    fn open_capture(
        &self,
        spec: CaptureSpec,
        sink: ArcChannelSink<f32>,
    ) -> Result<Self::Capture, RibbleWhisperError> {
        let (capture_sender, capture_receiver) = get_channel(1);

        let request = AudioCaptureRequest::Open(spec, sink, capture_sender);

        if let Err(e) = self.request_sender.send(request) {
            log::error!("Cannot send audio capture request to main thread.\n\
            Error: {}\n\
            Error source: {:#?}", &e, e.source());
            let err = RibbleWhisperError::DeviceError("Cannot obtain audio device.".to_string());
            return Err(err);
        }


        capture_receiver.recv().map_err(|_e| {
            RibbleWhisperError::DeviceError(
                "Backend did not respond to capture request".to_string(),
            )
        })?
    }

    fn close_capture(&self, capture: Self::Capture) {
        let id = capture.device_id;
        let request = AudioCaptureRequest::Close(id);

        // The only case where this should ever error out is if the main thread
        // has either panicked or has closed.
        if let Err(e) = self.request_sender.send(request) {
            log::error!("Cannot send audio close request to main thread.\n\
            Error: {}\n\
            Error source: {:#?}", &e, e.source());
        }
    }
}

// Since SDL2 uses a Mutex to guard calls to pause/resume audio capture, for all intents and
// purposes, the inner Sdl2 capture should be considered Sync.
//
// To guarantee thread-safety, a copy of this capture should always exist on the main thread and
// must only be dropped on the main thread.
#[derive(Clone)]
pub struct SharedSdl2Capture<S: SampleSink> {
    device_id: usize,
    inner: Arc<Sdl2Capture<S>>,
}

impl<S: SampleSink> SharedSdl2Capture<S> {
    pub fn new(device_id: usize, sdl_capture: Arc<Sdl2Capture<S>>) -> Self {
        Self {
            device_id,
            inner: sdl_capture,
        }
    }

    // NOTE: this can probably be removed --> the check in the UI operates on an Arc pointer
    // instead of a SharedCapture.
    pub fn last_ref(&self) -> bool {
        Arc::strong_count(&self.inner) == 1
    }
}

unsafe impl<S: SampleSink> Sync for SharedSdl2Capture<S> {}
unsafe impl<S: SampleSink> Send for SharedSdl2Capture<S> {}

impl<S: SampleSink> MicCapture for SharedSdl2Capture<S> {
    fn play(&self) {
        self.inner.play()
    }

    fn pause(&self) {
        self.inner.pause()
    }

    fn sample_rate(&self) -> usize {
        self.inner.sample_rate()
    }

    fn format(&self) -> ribble_whisper::audio::microphone::RibbleAudioFormat {
        self.inner.format()
    }

    fn channels(&self) -> u8 {
        self.inner.channels()
    }

    fn buffer_size(&self) -> usize {
        self.inner.buffer_size()
    }
}
