use crate::controller::ConsoleMessage;
use crate::controller::RibbleMessage;
use crate::controller::WorkRequest;
use crate::controller::{Bus, CompletedRecordingJobs};
use crate::utils::errors::RibbleError;
use crate::utils::recorder_configs::RibbleRecordingConfigs;
use crate::utils::recorder_configs::RibbleRecordingExportFormat;
use hound::{WavReader, WavSpec, WavWriter};
use indexmap::IndexMap;
use parking_lot::RwLock;
use ribble_whisper::audio::pcm::IntoPcmS16;
use ribble_whisper::utils::{Receiver, Sender};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

pub(super) struct WriteRequest {
    receiver: Receiver<Arc<[f32]>>,
    spec: RibbleRecordingConfigs,
}

// TODO: this might need to spawn a debouncer thread;
// Since the model folder is accessible via the UI,
// it is possible for a user to navigate to the recordings folder
// and delete a file.
//
// If they try and load said file after it's deleted, they will get
// annoying UI, but the missing file will be removed by the next repaint.
//
// Possible solutions:
// - Debouncer -> guaranteed coherent, runs on a bg thread, not super expensive but
//   extra thread overhead.
// - Filtering (check for file) -> likely coherent, incurs memory allocation each read.
// - Toast + repaint -> informs, very cheap, more UI friction than debouncer

impl WriteRequest {
    // NOTE: instead of just copying over the passed configs, this should take a RecordingConfigs
    // that's been constructed by using RibbleRecordingConfigs::from_mic_capture(...);
    // SEE ABOVE TODO: change to confirmed so that all spec details are known + NO AUTO.
    pub(super) fn new(receiver: Receiver<Arc<[f32]>>, spec: RibbleRecordingConfigs) -> Self {
        Self { receiver, spec }
    }

    pub(super) fn unpack(self) -> (Receiver<Arc<[f32]>>, RibbleRecordingConfigs) {
        (self.receiver, self.spec)
    }
}

// NOTE: this is only send when using mpmc (crossbeam)
// If for whatever reason the std::mpsc is required,
// this will need to have a lock on the Receiver
struct WriterEngineState {
    ticket: AtomicUsize,
    clearing: AtomicBool,
    latest_exists: AtomicBool,
    data_directory: PathBuf,
    completed_jobs: RwLock<IndexMap<Arc<str>, CompletedRecordingJobs>>,
    incoming_jobs: Receiver<WriteRequest>,
    // This is just for spawning a write loop - the outer WriterEngine has to handle sending clear
    // jobs.
    work_request_sender: Sender<WorkRequest>,
}

impl WriterEngineState {
    const DEFAULT_CACHE_SIZE: usize = 5;
    const TMP_FILE: &'static str = "tmp_recording";
    const FILE_EXTENSION: &'static str = ".wav";
    fn new(data_directory: PathBuf, incoming_jobs: Receiver<WriteRequest>, bus: &Bus) -> Self {
        let ticket = AtomicUsize::new(0);
        let clearing = AtomicBool::new(false);
        let latest_exists = AtomicBool::new(false);
        let completed_jobs = RwLock::new(IndexMap::with_capacity(Self::DEFAULT_CACHE_SIZE));

        Self {
            ticket,
            clearing,
            latest_exists,
            data_directory,
            completed_jobs,
            incoming_jobs,
            work_request_sender: bus.work_request_sender(),
        }
    }
    fn is_clearing(&self) -> bool {
        self.clearing.load(Ordering::Acquire)
    }

    fn try_get_latest(&self) -> Option<PathBuf> {
        // Try and get the last inserted key
        let latest = self
            .completed_jobs
            .read()
            .last()
            .and_then(|(file_name, _)| Some(self.data_directory.join(file_name.as_ref())));

        // If it doesn't exist, internally update the status and return the Option.
        if latest.is_none() {
            self.latest_exists.store(false, Ordering::Release);
        }

        latest
    }

    // NOTE: if this is causing issues with the UI loop, return an option instead and use heuristics.
    // If the offline/recording has been run at least once, there must exist a recording that can
    // be loaded.
    fn get_num_completed(&self) -> usize {
        self.completed_jobs.read().len()
    }

    fn get_recording_path(&self, file_name: Arc<str>) -> Option<PathBuf> {
        let mut map = self.completed_jobs.write();
        if !map.contains_key(&file_name) {
            return None;
        }

        let expected_path = self.data_directory.join(file_name.as_ref());
        if !expected_path.is_file() {
            // Remove the broken link from the map if it no longer exists.
            map.shift_remove(&file_name);
            if map.is_empty() {
                self.latest_exists.store(false, Ordering::Release);
            }
            None
        } else {
            Some(expected_path)
        }
    }

    // NOTE: the comments below might be untrue -> if this does have to end up returning an error,
    // reset the clearing flag in an or_else clause.
    fn clear_cache(&self) -> Result<RibbleMessage, RibbleError> {
        self.clearing.store(true, Ordering::Release);
        let mut completed_jobs = self.completed_jobs.write();

        for file in completed_jobs.keys() {
            let file_path = self.data_directory.join(file.as_ref());
            if let Ok(exists) = std::fs::exists(file_path.as_path()) {
                // Don't care if the path is a directory (it should never, ever be one)
                // Don't care if the file is already gone (the entry will get deleted from the map)
                //
                // If a user lacks permission to remove the file, then they're going to have a lot
                // of trouble running this application - but it's not an error.
                // When the app re-launches, this will just clobber any existing temporary files
                // for recordings - i.e. let whomever can clear the cache files, clear the cache
                // files.
                if exists {
                    let _ = std::fs::remove_file(file_path.as_path());
                }
            }
        }

        self.latest_exists.store(false, Ordering::Release);
        // Then empty the hashmap and reset the accumulator.
        completed_jobs.clear();
        self.ticket.store(0, Ordering::Release);

        let console_message = ConsoleMessage::Status("Recording cache cleared.".to_string());
        let ribble_message = RibbleMessage::Console(console_message);

        self.clearing.store(false, Ordering::Release);
        Ok(ribble_message)
    }

    fn export_recording(
        &self,
        // Since the outer API has to CoW, just take the path.
        outfile_path: PathBuf,
        // ibid -> take a clone of the shared string.
        key: Arc<str>,
        format: RibbleRecordingExportFormat,
    ) -> Result<RibbleMessage, RibbleError> {
        let tmp_file_path = self.data_directory.join(key.as_ref());
        let check_tmp_file = std::fs::exists(tmp_file_path.as_path());
        if check_tmp_file.is_err() || !check_tmp_file? {
            let error = std::io::Error::from(std::io::ErrorKind::NotFound);
            return Err(RibbleError::IOError(error));
        }
        // If it's already in floating point, then this can be a direct copy.
        if matches!(format, RibbleRecordingExportFormat::F32) {
            std::fs::copy(tmp_file_path.as_path(), outfile_path.as_path())?;

            let console_message =
                ConsoleMessage::Status(format!("Saved recording to {}!", outfile_path.display()));
            let ribble_message = RibbleMessage::Console(console_message);
            Ok(ribble_message)
        } else {
            let read_guard = self.completed_jobs.read();

            let job = read_guard.get(key.as_ref()).ok_or(RibbleError::Core(
                "Temp recording metadata not found.".to_string(),
            ))?;

            let sample_rate = job.sample_rate();
            let channels = job.channels();
            let spec = WavSpec {
                channels: channels as u16,
                sample_rate: sample_rate as u32,
                bits_per_sample: 16,
                sample_format: format.into(),
            };

            // Open a reader to read in the file to a buffer
            let mut reader = WavReader::open(tmp_file_path.as_path())?;

            let int_audio = reader
                .samples::<f32>()
                .map(|sample| sample.map(|f| f.into_pcm_s16()))
                .collect::<Result<Vec<i16>, _>>()?;

            // Open a writer to read the new file out.
            let mut writer = WavWriter::create(outfile_path.as_path(), spec)?;
            for int_sample in int_audio {
                writer.write_sample(int_sample)?
            }

            writer.finalize()?;
            let console_message =
                ConsoleMessage::Status(format!("Saved recording to {}!", outfile_path.display()));
            let ribble_message = RibbleMessage::Console(console_message);
            Ok(ribble_message)
        }
    }

    fn handle_new_request(&self, request: WriteRequest) -> Result<RibbleMessage, RibbleError> {
        // Unpack the request
        let (receiver, spec) = request.unpack();

        // The files should look like "tmp<ticket_no>.wav"
        let ticket_no = self.ticket.fetch_add(1, Ordering::AcqRel);
        let tmp_name = Self::TMP_FILE;
        let ext = Self::FILE_EXTENSION;
        let file_name = format!("{tmp_name}{ticket_no}.{ext}");
        let path = self.data_directory.join(&file_name);

        // Make a new WavWriter
        let wav_spec = spec.into_wav_spec(RibbleRecordingExportFormat::F32)?;
        // TODO: write an errors hook for hound errors.
        let mut writer = WavWriter::create(path, wav_spec)?;
        // NOTE: SDL (current backend sends interleaved data)
        // Wav is also interleaved, so this can just automatically write samples

        let mut num_floats = 0usize;
        while let Ok(samples) = receiver.recv() {
            for sample in samples.iter().copied() {
                writer.write_sample(sample)?;
            }

            num_floats += samples.len();
        }

        let total_duration_in_seconds = writer.duration() / wav_spec.sample_rate;
        let total_duration = Duration::from_secs(total_duration_in_seconds as u64);
        writer.finalize()?;
        let file_size_estimate = num_floats * size_of::<f32>();

        // Make a new entry in the completed jobs queue.
        let mut job_bank = self.completed_jobs.write();
        job_bank.insert(
            Arc::from(file_name),
            CompletedRecordingJobs {
                total_duration,
                file_size_estimate,
                sample_rate: wav_spec.sample_rate as usize,
                channels: wav_spec.channels as usize,
            },
        );

        // Format HH:MM:SS
        let secs = total_duration.as_secs();
        let seconds = secs % 60;
        let minutes = (secs / 60) % 60;
        let hours = (secs / 60) / 60;

        let console_message = ConsoleMessage::Status(format!(
            "Finished Recording. Total duration: {hours}:{minutes}:{seconds}"
        ));

        // Update the latest_exists flag --> if the transcription was written, it has to exist
        // and be accessible.
        self.latest_exists.fetch_or(true, Ordering::AcqRel);
        let ribble_message = RibbleMessage::Console(console_message);
        Ok(ribble_message)
    }
}

pub(super) struct WriterEngine {
    inner: Arc<WriterEngineState>,
    request_polling_thread: Option<JoinHandle<Result<(), RibbleError>>>,
    // NOTE: this isn't the absolute best architecture decision, but it solves the need for 'static
    // lifetimes when spawning threads for clearing/exporting.
    work_sender: Sender<WorkRequest>,
}

impl WriterEngine {
    pub(super) fn new(
        data_directory: PathBuf,
        incoming_jobs: Receiver<WriteRequest>,
        bus: &Bus,
    ) -> Self {
        let inner = Arc::new(WriterEngineState::new(data_directory, incoming_jobs, bus));
        let thread_inner = Arc::clone(&inner);
        // NOTE: It would probably be an optimization to just pre-clone the pointer; if it's
        // genuinely an issue, that's low-hanging fruit.
        let polling_thread = std::thread::spawn(move || {
            while let Ok(request) = thread_inner.incoming_jobs.recv() {
                let request_handler_inner = Arc::clone(&thread_inner);
                let handle_request =
                    std::thread::spawn(move || request_handler_inner.handle_new_request(request));

                // TODO: this doesn't work -> The thread needs to be spawned here
                let work_request = WorkRequest::Short(handle_request);

                if thread_inner.work_request_sender.send(work_request).is_err() {
                    todo!("LOGGING");
                    break;
                }
            }

            Ok(())
        });

        Self {
            inner,
            request_polling_thread: Some(polling_thread),
            work_sender: bus.work_request_sender(),
        }
    }

    // TODO: take a copy of the work_request sender and send from here.
    //
    // NOTE: Send the key in if the user wants to export a recording.
    // NOTE TWICE: remove .into() once RibbleAppError has been removed.
    pub(super) fn export_recording(
        &self,
        out_path: PathBuf,
        // This is the key -> clone it in the UI and take ownership of the pointer
        job_file_name: Arc<str>,
        output_format: RibbleRecordingExportFormat,
    ) {
        let thread_inner = Arc::clone(&self.inner);
        // NOTE: these either need to be static references, or copy-on-write.
        // Since it's not expected to happen often, CoW is most likely the easiest solution to
        // avoid atomic shared pointers.
        let worker = std::thread::spawn(move || {
            thread_inner.export_recording(out_path, job_file_name, output_format)
        });

        let work_request = WorkRequest::Short(worker);

        if self.work_sender.send(work_request).is_err() {
            todo!("LOGGING");
        }
    }

    // NOTE: since the IndexMap preserves ordering based on insertion order, this
    // Needs to be reversed so that the information is presented most-recent to least-recent
    // IF this hapeens to be causing any significant lag, reverse the iterator in the ui.
    pub(super) fn try_get_completed_jobs(
        &self,
        copy_buffer: &mut Vec<(Arc<str>, CompletedRecordingJobs)>,
    ) {
        if let Some(jobs) = self.inner.completed_jobs.try_read() {
            copy_buffer.clear();
            copy_buffer.extend(
                jobs.iter()
                    .map(|(file_name, metadata)| (Arc::clone(file_name), metadata.clone())),
            );
            copy_buffer.reverse();
        }
    }

    pub(super) fn latest_exists(&self) -> bool {
        self.inner.latest_exists.load(Ordering::Acquire)
    }

    // NOTE: use this if a user wants to re-transcribe their last recording.
    // In the UI, label this button: Re-Transcribe latest (recording)
    // If this is none, that means either writing hasn't finished, or there are no recordings.
    // This is not necessarily an error.
    pub(super) fn try_get_latest(&self) -> Option<PathBuf> {
        self.inner.try_get_latest()
    }

    // NOTE: if this is causing noticeable UI jank with the lock contention,
    // return an option and respond accordingly in the UI.
    pub(super) fn get_num_completed(&self) -> usize {
        self.inner.get_num_completed()
    }

    pub(super) fn get_recording_path(&self, file_name: Arc<str>) -> Option<PathBuf> {
        self.inner.get_recording_path(file_name)
    }

    // Use this to disable a clear cache button in the UI thread.
    pub(super) fn is_clearing(&self) -> bool {
        self.inner.is_clearing()
    }

    pub(super) fn clear_cache(&self) {
        // TODO: if guarding against grandma clicks isn't necessary, remove this mechanism.
        if self.inner.is_clearing() {
            return;
        }

        let thread_inner = Arc::clone(&self.inner);
        let worker = std::thread::spawn(move || thread_inner.clear_cache());

        let work_request = WorkRequest::Short(worker);
        // TODO: TEST THE BLOCKING -> If this encounters any blocking, the short queue needs to be
        // increased.
        if self.work_sender.send(work_request).is_err() {
            todo!("LOGGING");
        }
    }
}

impl Drop for WriterEngine {
    fn drop(&mut self) {
        if let Some(handle) = self.request_polling_thread.take() {
            handle
                .join()
                .expect("The Writer thread is not expected to panic and should run without issues.")
                .expect("I'm not sure what the errors for this might be")
        }
        // Also, clear the cache.
        let _ = self.inner.clear_cache();
    }
}
