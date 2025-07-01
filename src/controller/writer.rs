// Basic idea: spawn a thread on construction time that waits for new requests for writing.
// Upon one, send a worker job (via message queues once the kernel stuff is refactored out).
// Store a limited number of temporary file recordings (keep an accumulator modulo num recordings).
use crate::controller::console::ConsoleMessage;
use crate::controller::{RibbleMessage, RibbleWorkerHandle};
use crate::utils::errors::RibbleError;
use crate::utils::recorder_configs::RibbleRecordingConfigs;
use crate::utils::recorder_configs::RibbleRecordingExportFormat;
use hound::{WavReader, WavSpec, WavWriter};
use indexmap::IndexMap;
use parking_lot::RwLock;
use ribble_whisper::audio::pcm::IntoPcmS16;
use ribble_whisper::utils::{Receiver, Sender};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

pub(super) struct WriteRequest {
    receiver: Receiver<Arc<[f32]>>,
    spec: RibbleRecordingConfigs,
}

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

// Not sure, maybe sort by start time?
#[derive(Copy, Clone)]
struct CompletedJobs {
    // This can probably just be accumulated.
    file_size_estimate: usize,
    total_duration: Duration,
    channels: usize,
    sample_rate: usize,
}

// NOTE: this is only send when using mpmc (crossbeam)
// If for whatever reason the std::mpsc is required,
// this will need to have a lock on the Receiver
struct WriterEngineState {
    ticket: AtomicUsize,
    clearing: AtomicBool,
    data_directory: PathBuf,
    completed_jobs: RwLock<IndexMap<String, CompletedJobs>>,
    incoming_jobs: Receiver<WriteRequest>,
    worker_engine_channel: Sender<RibbleWorkerHandle>,
}

impl WriterEngineState {
    const DEFAULT_CACHE_SIZE: usize = 5;
    const TMP_FILE: &'static str = "tmp_recording";
    const FILE_EXTENSION: &'static str = ".wav";
    fn new(
        data_directory: PathBuf,
        incoming_jobs: Receiver<WriteRequest>,
        worker_engine_channel: Sender<RibbleWorkerHandle>,
    ) -> Self {
        let ticket = AtomicUsize::new(0);
        let clearing = AtomicBool::new(false);
        let completed_jobs = RwLock::new(IndexMap::with_capacity(Self::DEFAULT_CACHE_SIZE));
        Self {
            ticket,
            clearing,
            data_directory,
            completed_jobs,
            incoming_jobs,
            worker_engine_channel,
        }
    }
    fn is_clearing(&self) -> bool {
        self.clearing.load(Ordering::Acquire)
    }

    fn try_get_latest(&self) -> Result<PathBuf, RibbleError> {
        // Try and get the last inserted key
        let latest = self.completed_jobs.read().last().ok_or(RibbleError::Core(
            "Latest cached recording missing!".to_string(),
        ))?;
        Ok(self.data_directory.join(latest))
    }

    fn clear_cache(&self) -> Result<RibbleMessage, RibbleError> {
        self.clearing.store(true, Ordering::Release);
        let mut completed_jobs = self.completed_jobs.write();

        for file in completed_jobs.keys() {
            let file_path = self.data_directory.join(file);
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
        // Then empty the hashmap and reset the accumulator.
        completed_jobs.clear();
        self.ticket.store(0, Ordering::Release);

        let console_message = ConsoleMessage::Status("Recording cache cleared.".to_string());
        let ribble_message = RibbleMessage::Console(console_message);

        self.clearing.store(false, Ordering::Release);
        Ok(ribble_message)
    }

    fn export_file(
        &self,
        outfile_path: &Path,
        key: &String,
        format: RibbleRecordingExportFormat,
    ) -> Result<RibbleMessage, RibbleError> {
        let tmp_file_path = self.data_directory.join(key);
        let check_tmp_file = std::fs::exists(tmp_file_path.as_path());
        if check_tmp_file.is_err() || !check_tmp_file? {
            let error = std::io::Error::from(std::io::ErrorKind::NotFound);
            return Err(RibbleError::IOError(error));
        }
        // If it's already in floating point, then this can be a direct copy.
        if matches!(format, RibbleRecordingExportFormat::F32) {
            std::fs::copy(tmp_file_path.as_path(), outfile_path)?;

            let console_message =
                ConsoleMessage::Status(format!("Saved recording to {outfile_path}!"));
            let ribble_message = RibbleMessage::Console(console_message);
            Ok(ribble_message)
        } else {
            // Otherwise, convert to S16 PCM audio and write out.
            let job = self
                .completed_jobs
                .read()
                .get(key)
                .ok_or(RibbleError::Core(
                    "Temp recording metadata not found.".to_string(),
                ))?;

            let sample_rate = job.sample_rate;
            let channels = job.channels;
            let spec = WavSpec {
                channels: channels.into(),
                sample_rate: sample_rate.into(),
                bits_per_sample: 16,
                sample_format: format.into(),
            };

            // Open a reader to read in the file to a buffer
            let mut reader = WavReader::open(tmp_file_path.as_path())?;

            let int_audio = reader
                .samples::<f32>()
                .map(|sample| {
                    sample
                        .map(|sample| sample.into_pcm_s16())
                        // TODO: Fix this once hound hook is implemented.
                        .map_err(|sample| {
                            RibbleError::Core(
                                "Cached recording does not match expected spec.".to_string(),
                            )
                        })
                })
                .collect::<Result<Vec<i16>, RibbleError>>()?;

            // Open a writer to read the new file out.
            let mut writer = WavWriter::create(outfile_path, spec)?;
            for int_sample in int_audio {
                writer.write_sample(int_sample)?
            }
            writer.finalize()?;
            let console_message =
                ConsoleMessage::Status(format!("Saved recording to {outfile_path}!"));
            let ribble_message = RibbleMessage::Console(console_message);
            Ok(ribble_message)
        }
    }

    // NOTE: this might need some more tlc.  Since there's no way to return
    // feedback to the caller about the status of this job until it's joined,
    // start the thread early before doing anything that could return an error.
    // The WorkerEngine has tools to provide information about the execution.
    fn handle_new_request(&self, request: WriteRequest) -> RibbleWorkerHandle {
        std::thread::spawn(move || {
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
                file_name,
                CompletedJobs {
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
            let ribble_message = RibbleMessage::Console(console_message);
            Ok(ribble_message)
        })
    }
}

pub(super) struct WriterEngine {
    inner: Arc<WriterEngineState>,
    request_polling_thread: Option<JoinHandle<Result<(), RibbleError>>>,
}

// TODO: take in a bus instead of the explicit sender queue
impl WriterEngine {
    fn new(
        data_directory: PathBuf,
        incoming_jobs: Receiver<WriteRequest>,
        worker_engine_channel: Sender<RibbleWorkerHandle>,
    ) -> Self {
        let inner = Arc::new(WriterEngineState::new(
            data_directory,
            incoming_jobs,
            worker_engine_channel,
        ));
        let thread_inner = Arc::clone(&inner);
        let polling_thread = std::thread::spawn(move || {
            while let Ok(request) = thread_inner.incoming_jobs.recv() {
                // NOTE: if there are problems with send + sync, turn into a static
                // method/utility function.
                if thread_inner
                    .worker_engine_channel
                    .send(thread_inner.handle_new_request(request))
                    .is_err()
                {
                    // TODO: do some sort of logging -> this would mean that the
                    // WorkerEngine has deallocated--and this should be the end of the
                    // application, but in case it's not, this should be logged to pinpoint
                    // errors.

                    break;
                }
            }

            Ok(())
        });

        Self {
            inner,
            request_polling_thread: Some(polling_thread),
        }
    }

    // NOTE: Send the key in if the user wants to export a recording.
    // NOTE TWICE: remove .into() once RibbleAppError has been removed.
    pub(super) fn export(
        &self,
        out_path: &Path,
        job_file_name: &String,
        output_format: RibbleRecordingExportFormat,
    ) -> RibbleWorkerHandle {
        let thread_inner = Arc::clone(&self.inner);
        std::thread::spawn(move || thread_inner.export_file(out_path, job_file_name, output_format))
            .into()
    }

    // NOTE: since the IndexMap preserves ordering based on insertion order, this
    // Needs to be reversed so that the information is presented most-recent to least-recent
    pub(super) fn try_get_completed_jobs(&self, copy_buffer: &mut Vec<(String, CompletedJobs)>) {
        if let Some(jobs) = self.inner.completed_jobs.try_read() {
            copy_buffer.clear();
            copy_buffer.extend(jobs.iter().copied());
            copy_buffer.reverse();
        }
    }

    // NOTE: use this if a user wants to re-transcribe their last recording.
    // In the UI, label this button: Re-Transcribe latest (recording)
    // (if this fails, it means there are no recordings, or there was an issue writing to a temp file).
    pub(super) fn try_get_latest(&self) -> Result<PathBuf, RibbleError> {
        self.inner.try_get_latest()
    }

    pub(super) fn get_recording_path(&self, file_name: &String) -> Option<&String> {
        todo!("Finish this.")
        // Do the logic in inner.
        // Test the key (internal temp file_name -> is in the completed jobs list)
        // If the key exists on disk, return Some(file_name)
        // The caller can decide what to do with the file_name.
        // (This most likely involves a clone, but the frequency with which it happens is low enough)
        // And the strings are very, very small that the allocation should be nearly free.
    }

    // Use this to disable a clear cache button in the UI thread.
    pub(super) fn is_clearing(&self) -> bool {
        self.inner.is_clearing()
    }

    // NOTE: remove .into() once RibbleAppError has been removed.
    pub(super) fn clear_cache(&self) -> Option<RibbleWorkerHandle> {
        // TODO: if guarding against grandma clicks isn't necessary, remove this mechanism.
        if self.inner.is_clearing() {
            return None;
        }
        let thread_inner = Arc::clone(&self.inner);
        Some(std::thread::spawn(move || {
            thread_inner.clear_cache().into()
        }))
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
