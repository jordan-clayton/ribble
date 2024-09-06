use std::{
    fs::File,
    io::{self, BufReader, BufWriter, ErrorKind, Write},
    path::{Path, PathBuf},
};

use hound::{Sample, WavReader, WavSpec, WavWriter};
use symphonia::{
    core::{
        audio::SampleBuffer,
        codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions},
        errors::Error,
        formats::{FormatOptions, FormatReader},
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint,
    },
    default::get_probe,
};

use crate::utils::{
    constants,
    errors::{WhisperAppError, WhisperAppErrorType},
};

fn qualify_path(dir: &Path) -> PathBuf {
    let mut path = dir.to_path_buf();
    path.push(constants::TEMP_FILE);
    path
}

pub fn delete_temporary_audio_file() -> io::Result<()> {
    let data_dir = eframe::storage_dir(constants::APP_ID).expect("Failed to get data directory.");
    let file_path = get_temp_file_path(&data_dir);
    std::fs::remove_file(&file_path)?;
    Ok(())
}

pub fn copy_data(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::copy(from, to)?;
    Ok(())
}

pub fn get_temp_file_path(data_dir: &Path) -> PathBuf {
    qualify_path(data_dir)
}

pub fn get_tmp_file_writer(
    data_dir: &Path,
    spec: &WavSpec,
) -> hound::Result<WavWriter<BufWriter<File>>> {
    let path = qualify_path(data_dir);
    get_wav_output_writer(path.as_path(), spec)
}

pub fn get_tmp_file_reader(data_dir: &Path) -> hound::Result<WavReader<BufReader<File>>> {
    let path = qualify_path(data_dir);
    get_wav_input_reader(path.as_path())
}

pub fn get_wav_output_writer(
    path: &Path,
    spec: &WavSpec,
) -> hound::Result<WavWriter<BufWriter<File>>> {
    hound::WavWriter::create(path, *spec)
}

pub fn get_wav_input_reader(path: &Path) -> hound::Result<WavReader<BufReader<File>>> {
    hound::WavReader::open(path)
}

pub fn get_audio_reader(
    path: &Path,
) -> Result<(u32, Box<dyn FormatReader>, Box<dyn Decoder>), WhisperAppError> {
    let src = File::open(path);
    if let Err(_e) = src.as_ref() {
        let error = WhisperAppError::new(
            WhisperAppErrorType::ParameterError,
            format!("Invalid path: {:?}", path),
        );
        return Err(error);
    }

    let src = src.unwrap();
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let mut hint = Hint::new();

    let ext = path.extension();
    if ext.is_none() {
        let error = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Failed to parse file extension, {:?}", path),
        );
        return Err(error);
    }
    let ext = ext.unwrap();

    let ext = ext.to_str();
    if ext.is_none() {
        let error = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Failed to convert OsStr {:?} to str", ext),
        );
        return Err(error);
    }

    let ext = ext.unwrap();

    hint.with_extension(ext);

    let meta_opts: MetadataOptions = Default::default();
    let fmt_opts: FormatOptions = Default::default();

    let probe = get_probe().format(&hint, mss, &fmt_opts, &meta_opts);
    if let Err(e) = probe.as_ref() {
        let error = WhisperAppError::new(
            WhisperAppErrorType::ParameterError,
            format!("Unsupported file format. Error: {}", e),
        );
        return Err(error);
    }

    let probe = probe.unwrap();

    let format = probe.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL);
    if track.is_none() {
        let error = WhisperAppError::new(
            WhisperAppErrorType::ParameterError,
            format!("Failed to find an audio track in {:?}", path),
        );
        return Err(error);
    }

    let track = track.unwrap();
    let dec_opts: DecoderOptions = Default::default();
    let decoder = symphonia::default::get_codecs().make(&track.codec_params, &dec_opts);

    if let Err(e) = decoder.as_ref() {
        let error = WhisperAppError::new(
            WhisperAppErrorType::ParameterError,
            format!("Unsupported format. Error: {}", e),
        );
        return Err(error);
    }

    let decoder = decoder.unwrap();

    let track_id = track.id;

    Ok((track_id, format, decoder))
}

// progress closure = (samples decoded so far, in bytes);
pub fn decode_audio(
    id: u32,
    mut reader: Box<dyn FormatReader>,
    mut decoder: Box<dyn Decoder>,
    mut progress_closure: Option<impl FnMut(usize)>,
) -> Result<Vec<f32>, WhisperAppError> {
    let mut samples = vec![];
    let mut sample_buf = None;

    loop {
        let packet = match reader.next_packet() {
            Ok(p) => p,
            Err(Error::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(Error::IoError(e)) => {
                if e.kind() == ErrorKind::UnexpectedEof {
                    break;
                }
                let error = WhisperAppError::new(
                    WhisperAppErrorType::Unknown,
                    format!("Unable to decode audio samples. Error: {}", e),
                );
                return Err(error);
            }
            Err(e) => {
                let error = WhisperAppError::new(
                    WhisperAppErrorType::Unknown,
                    format!("Unable to decode audio samples. Error: {}", e),
                );
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
                let num_channels = audio_buf.spec().channels.iter().count();

                let in_mono = num_channels == 1;

                if sample_buf.is_none() {
                    let spec = *audio_buf.spec();
                    let duration = audio_buf.capacity() as u64;
                    sample_buf = Some(SampleBuffer::<f32>::new(duration, spec));
                }

                if let Some(buf) = sample_buf.as_mut() {
                    buf.copy_interleaved_ref(audio_buf);
                    let new_audio = if in_mono {
                        buf.samples().to_vec()
                    } else {
                        let audio = buf.samples();

                        whisper_realtime::whisper_rs::convert_stereo_to_mono_audio(audio)
                            .expect("Failed to convert to mono")
                    };

                    samples.extend_from_slice(&new_audio);
                    if let Some(p) = progress_closure.as_mut() {
                        let p_size = samples.len() * size_of::<f32>();
                        p(p_size);
                    }
                }
            }
            Err(Error::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(Error::DecodeError(e)) => {
                let error = WhisperAppError::new(
                    WhisperAppErrorType::ParameterError,
                    format!("Decode failure. {}", e),
                );
                return Err(error);
            }
            Err(Error::IoError(e)) => {
                if e.kind() == ErrorKind::UnexpectedEof {
                    break;
                }
                let error = WhisperAppError::new(
                    WhisperAppErrorType::ParameterError,
                    format!("IO Error. {}", e),
                );
                return Err(error);
            }
            Err(e) => {
                let error = WhisperAppError::new(
                    WhisperAppErrorType::ParameterError,
                    format!("Decode failure. {}", e),
                );
                return Err(error);
            }
        }
    }
    Ok(samples)
}

pub fn save_transcription(
    file_path: &Path,
    transcript: &str,
    mut progress_callback: Option<impl FnMut(usize)>,
) -> Result<(), WhisperAppError> {
    let mut byte_string = transcript.as_bytes();
    let file = File::create(file_path);

    if let Err(e) = file.as_ref() {
        let error = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Failed to write to file: {:?}. Error: {}", file_path, e),
        );
        return Err(error);
    }

    let file = file.unwrap();
    let mut writer = BufWriter::new(file);
    let mut total_bytes_written = 0;
    while !byte_string.is_empty() {
        match writer.write(byte_string) {
            Ok(0) => {
                let error = WhisperAppError::new(
                    WhisperAppErrorType::IOError,
                    format!("Unexpected EOF, cannot write to: {:?}", file_path),
                );
                return Err(error);
            }
            Ok(n) => {
                if let Some(c) = progress_callback.as_mut() {
                    total_bytes_written += n;
                    c(total_bytes_written)
                }
                byte_string = &byte_string[n..];
            }
            Err(e) => {
                if e.kind() == ErrorKind::Interrupted {
                    continue;
                }

                let error = WhisperAppError::new(
                    WhisperAppErrorType::IOError,
                    format!("Failed to write to file: {:?}.  Error: {}", file_path, e),
                );
                return Err(error);
            }
        }
    }

    let flushed = writer.flush();
    if let Err(e) = flushed.as_ref() {
        let error = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Failed to write to file: {:?}. Error: {}", file_path, e),
        );
        return Err(error);
    }

    Ok(())
}

pub fn write_audio_sample<T: Sample + Clone>(
    sample: &[T],
    writer: &mut WavWriter<BufWriter<File>>,
    progress_callback: Option<impl FnMut(usize) + Send + Sync + 'static>,
) {
    let len = sample.len();

    match progress_callback {
        None => {
            for i in 0..len {
                writer
                    .write_sample(sample[i].clone())
                    .expect("Failed to write sample.");
            }
        }
        Some(mut c) => {
            for i in 0..len {
                writer
                    .write_sample(sample[i].clone())
                    .expect("Failed to write sample");
                c(i)
            }
        }
    };
}
