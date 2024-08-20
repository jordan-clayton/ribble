use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

use hound::{Sample, WavReader, WavSpec, WavWriter};
use symphonia::core::audio::{Layout, SampleBuffer};
use symphonia::core::codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::get_probe;
use whisper_realtime::errors::{WhisperRealtimeError, WhisperRealtimeErrorType};

use crate::utils::constants;

fn qualify_path(dir: &Path) -> PathBuf {
    let mut path = dir.to_path_buf();
    path.push(constants::TEMP_FILE);
    path
}

pub fn copy_data(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::copy(from, to)?;
    Ok(())
}

pub fn get_tmp_file_writer(data_dir: &Path, spec: &WavSpec) -> hound::Result<WavWriter<BufWriter<File>>> {
    let path = qualify_path(data_dir);
    get_wav_output_writer(path.as_path(), spec)
}

pub fn get_tmp_file_reader(data_dir: &Path) -> hound::Result<WavReader<BufReader<File>>> {
    let path = qualify_path(data_dir);
    get_wav_input_reader(path.as_path())
}

pub fn get_wav_output_writer(path: &Path, spec: &WavSpec) -> hound::Result<WavWriter<BufWriter<File>>> {
    hound::WavWriter::create(path, *spec)
}

pub fn get_wav_input_reader(path: &Path) -> hound::Result<WavReader<BufReader<File>>> {
    hound::WavReader::open(path)
}

pub fn get_audio_reader(path: &Path) -> Result<(u32, Box<dyn FormatReader>, Box<dyn Decoder>), WhisperRealtimeError> {
    let src = File::open(path);
    if let Err(_e) = src.as_ref() {
        let error = WhisperRealtimeError::new(WhisperRealtimeErrorType::ParameterError, format!("Invalid path: {:?}", path));
        return Err(error);
    }

    let src = src.unwrap();
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let mut hint = Hint::new();
    let ext = path.extension().expect(&format!("Failed to parse file extension, {:?}", path));
    let ext = ext.to_str().expect(&format!("Failed to convert OsStr {:?} to str", ext));
    hint.with_extension(ext);

    let meta_opts: MetadataOptions = Default::default();
    let fmt_opts: FormatOptions = Default::default();

    let probe = get_probe().format(&hint, mss, &fmt_opts, &meta_opts);

    if let Err(e) = probe.as_ref() {
        let error = WhisperRealtimeError::new(WhisperRealtimeErrorType::ParameterError, format!("Unsupported format: {}. Error: {}", ext, e));
        return Err(error);
    }

    let probe = probe.unwrap();


    let mut format = probe.format;
    let track = format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL);
    if track.is_none() {
        let error = WhisperRealtimeError::new(WhisperRealtimeErrorType::ParameterError, String::from("No supported audio tracks"));
        return Err(error);
    }

    let track = track.unwrap();
    let dec_opts: DecoderOptions = Default::default();
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &dec_opts);

    if let Err(e) = decoder.as_ref() {
        let error = WhisperRealtimeError::new(WhisperRealtimeErrorType::ParameterError, String::from("Unsupported codec"));
        return Err(error);
    }

    let mut decoder = decoder.unwrap();

    let track_id = track.id;

    Ok((track_id, format, decoder))
}

// This should be run on a separate thread.
pub fn decode_audio(id: u32, mut reader: Box<dyn FormatReader>, mut decoder: Box<dyn Decoder>, audio_closure: Option<impl Fn(&[f32])>) -> Result<(), WhisperRealtimeError> {
    let mut sample_buf = None;
    let channel_layout = decoder.codec_params().channel_layout.expect("Failed to get channel layout");
    let in_mono = match channel_layout {
        Layout::Mono => { true }
        Layout::Stereo => { false }
        _ => { panic!("Invalid channel format") }
    };

    loop {
        let packet = match reader.next_packet() {
            Ok(p) => { p }
            Err(Error::ResetRequired) => {
                // Track list has been changed -> needs to be re-examined and then the decode loop needs restarting.
                todo!();
            }
            Err(e) => {
                let error = WhisperRealtimeError::new(WhisperRealtimeErrorType::Unknown, String::from("Unable to decode audio samples."));
                return Err(error);
            }
        };

        // Consume metadata.
        while !reader.metadata().is_latest() {
            reader.metadata().pop();
        }

        // Skip over irrelevant tracks.
        if packet.track_id() != id {
            continue;
        }


        // Decode the packet into audio samples.
        match decoder.decode(&packet) {
            Ok(audio_buf) => {
                if sample_buf.is_none() {
                    let spec = *audio_buf.spec();
                    let duration = audio_buf.capacity() as u64;
                    sample_buf = Some(SampleBuffer::<f32>::new(duration, spec));
                }

                if let Some(buf) = sample_buf.as_mut() {
                    buf.copy_interleaved_ref(audio_buf);
                    let mut new_audio = buf.samples();

                    // Convert to mono for whisper.
                    if !in_mono {
                        let mono = whisper_realtime::whisper_rs::convert_stereo_to_mono_audio(new_audio).expect("Failed to convert to mono");
                        new_audio = mono.as_slice();
                        if let Some(closure) = audio_closure.as_ref() {
                            closure(new_audio);
                        }
                    } else {
                        if let Some(closure) = audio_closure.as_ref() {
                            closure(new_audio);
                        }
                    }
                }
            }
            // TODO: Determine whether to send this to console.
            // It might just be easier to panic the thread.
            Err(Error::DecodeError(_e)) => {
                let error = WhisperRealtimeError::new(WhisperRealtimeErrorType::ParameterError, String::from("Unsupported codec"));
                return Err(error);
                // TODO:
                // This should be separated to handle:
                // IoError, DecodeError
            }
            Err(_) => break,
        }
    }
    Ok(())
}

pub fn write_sample<T: Sample + Clone>(sample: &[T], writer: &mut WavWriter<BufWriter<File>>, progress_callback: Option<impl Fn(usize, usize) + Send + Sync + 'static>) {
    let len = sample.len();

    match progress_callback {
        None => {
            for i in 0..len {
                writer.write_sample(sample[i].clone()).expect("Failed to write sample.");
            }
        }
        Some(c) => {
            for i in 0..len {
                writer.write_sample(sample[i].clone()).expect("Failed to write sample");
                c(i, len)
            }
        }
    };
}
