use ribble_whisper::audio::audio_backend::CaptureSpec;
use ribble_whisper::audio::audio_backend::{AudioBackend, Sdl2Backend};
use ribble_whisper::audio::microphone::Sdl2Capture;
use ribble_whisper::audio::recorder::SampleSink;
use ribble_whisper::utils::errors::RibbleWhisperError;
use ribble_whisper::utils::{Sender, get_channel};

pub(crate) type AudioCaptureRequest = Box<dyn FnOnce(&Sdl2Backend) + Send>;

pub(crate) struct AudioBackendProxy {
    request_sender: Sender<AudioCaptureRequest>,
}

impl AudioBackendProxy {
    pub(crate) fn new(request_sender: Sender<AudioCaptureRequest>) -> Self {
        Self { request_sender }
    }
}
impl<S: SampleSink> AudioBackend<S> for AudioBackendProxy {
    type Capture = Sdl2Capture<S>;

    fn open_capture(
        &self,
        spec: CaptureSpec,
        sink: S,
    ) -> Result<Self::Capture, RibbleWhisperError> {
        let (capture_sender, capture_receiver) = get_channel(1);

        let request = Box::new(move |backend| {
            let _ = capture_sender.send(backend.open_capture(spec, sink));
        });

        let _ = self.request_sender.send(request);

        capture_receiver
            .recv()
            .map_err(RibbleWhisperError::DeviceError(
                "Backend did not respond to capture request".to_string(),
            ))?
    }
}
