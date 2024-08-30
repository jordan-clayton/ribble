use std::time::Duration;

// This has been tested to work with version 12.4.
pub const MIN_CUDA_MAJOR: i32 = 12;
pub const MIN_CUDA_MINOR: i32 = 4;
pub const BLANK_SEPARATOR: f32 = 12.0;

// TODO: Consider implementing user preferences pane.
pub const DEFAULT_CONSOLE_HISTORY_SIZE: usize = 35;

// 2 hours, in ms.
pub const MAX_REALTIME_TIMEOUT: u128 = Duration::new(7200, 0).as_millis();

pub const MIN_AUDIO_CHUNK_SIZE: usize = 2000;
pub const MAX_AUDIO_CHUNK_SIZE: usize = 30000;
pub const MAX_PHRASE_TIMEOUT: usize = 10000;
pub const MIN_VAD_SAMPLE_MS: usize = 100;
pub const MAX_VAD_SAMPLE_MS: usize = 1000;
pub const MIN_VAD_PROBABILITY: f32 = 0.5;
pub const MAX_VAD_PROBABILITY: f32 = 0.9;

// FFT CONSTANTS
// TODO: SETTLE ON PROPER VALUES
pub const FRAME_CONVERGENCE_ITERATIONS: usize = 1000;
pub const FRAME_CONVERGENCE_TOLERANCE: usize = 2;
pub const DEFAULT_F_LOWER: f32 = 100f32;
pub const DEFAULT_F_HIGHER: f32 = 4000f32;
pub const NUM_BUCKETS: usize = 32;
pub const SMOOTH_FACTOR: f32 = 8.0;
pub const MIN_LOW_FREQUENCY: f32 = 25.0;
pub const MAX_LOW_FREQUENCY: f32 = 300.0;
pub const MIN_HIGH_FREQUENCY: f32 = 330.0;
pub const MAX_HIGH_FREQUENCY: f32 = 20000.0;
pub const MAX_FFT_HEIGHT: f32 = 800.0;
pub const MIN_FFT_HEIGHT: f32 = 30.0;
pub const FFT_HEIGHT_EXPANSION: f32 = 20.0;
pub const FFT_MAX_HEIGHT_PROPORTION: f32 = 0.90;
pub const FFT_MIN_HEIGHT_PROPORTION: f32 = 0.10;
pub const FFT_MAX_WIDTH: f32 = 100.0;
pub const FFT_MIN_WIDTH: f32 = 8.0;
pub const FFT_GAIN: f32 = 30.0;

pub const APP_ID: &str = "WhisperGUI";
pub const QUALIFIER: &str = "com";
pub const ORGANIZATION: &str = "Jordan";

pub const CLEAR_MSG: &str = "[CLEAR]";
pub const GO_MSG: &str = "[START SPEAKING]";
pub const CLOSE_MSG: &str = "[APP CLOSE]";

pub const TEMP_FILE: &str = "tmp.wav";

pub const RECORDING_ANIMATION_TIMESCALE: f64 = 2.0;

// TODO: pick an appropriate livelock timeout.
pub const CHANNEL_TIMEOUT: Duration = Duration::from_millis(10);

pub const SLEEP_DURATION: Duration = Duration::from_millis(1);

pub const FROM_COLOR: egui::Rgba = egui::Rgba::from_rgba_premultiplied(0.0, 0.0, 0.0, 0.7);

pub const DESATURATION_MULTIPLIER: f32 = 0.5;
pub const MAX_WHISPER_THREADS: std::ffi::c_int = 8;

lazy_static::lazy_static! {
    pub static ref LANGUAGE_OPTIONS: std::collections::HashMap<&'static str, Option<String>> = maplit::hashmap!{
        "Auto" => None,
        "English" => Some(String::from("en")),
        "Mandarin" => Some(String::from("zh")),
        "German" => Some(String::from("de")),
        "Spanish" => Some(String::from("es")),
        "Russian" => Some(String::from("ru")),
        "Korean" => Some(String::from("ko")),
        "French" => Some(String::from("fr")),
        "Japanese" => Some(String::from("ja")),
        "Portuguese" => Some(String::from("pt")),
        "Turkish" => Some(String::from("tr")),
        "Polish" => Some(String::from("pl")),
        "Catalan" => Some(String::from("ca")),
        "Dutch" => Some(String::from("nl")),
        "Arabic" => Some(String::from("ar")),
        "Swedish" => Some(String::from("sv")),
        "Italian" => Some(String::from("it")),
        "Indonesian" => Some(String::from("id")),
        "Hindi" => Some(String::from("hi")),
        "Finnish" => Some(String::from("fi")),
        "Vietnamese" => Some(String::from("vi")),
        "Hebrew" => Some(String::from("he")),
        "Ukrainian" => Some(String::from("uk")),
        "Greek" => Some(String::from("el")),
        "Malay" => Some(String::from("ms")),
        "Czech" => Some(String::from("cs")),
        "Romanian" => Some(String::from("ro")),
        "Danish" => Some(String::from("da")),
        "Hungarian" => Some(String::from("hu")),
        "Tamil" => Some(String::from("ta")),
        "Norwegian" => Some(String::from("no")),
        "Thai" => Some(String::from("th")),
        "Urdu" => Some(String::from("ur")),
        "Croatian" => Some(String::from("hr")),
        "Bulgarian" => Some(String::from("bg")),
        "Lithuanian" => Some(String::from("lt")),
        "Latin" => Some(String::from("la")),
        "Maori" => Some(String::from("mi")),
        "Malayalam" => Some(String::from("ml")),
        "Welsh" => Some(String::from("cy")),
        "Slovak" => Some(String::from("sk")),
        "Telugu" => Some(String::from("te")),
        "Persian" => Some(String::from("fa")),
        "Latvian" => Some(String::from("lv")),
        "Bengali" => Some(String::from("bn")),
        "Serbian" => Some(String::from("sr")),
        "Azerbaijani" => Some(String::from("az")),
        "Slovenian" => Some(String::from("sl")),
        "Kannada" => Some(String::from("kn")),
        "Estonian" => Some(String::from("et")),
        "Macedonian" => Some(String::from("mk")),
        "Breton" => Some(String::from("br")),
        "Basque" => Some(String::from("eu")),
        "Icelandic" => Some(String::from("is")),
        "Armenian" => Some(String::from("hy")),
        "Nepali" => Some(String::from("ne")),
        "Mongolian" => Some(String::from("mn")),
        "Bosnian" => Some(String::from("bs")),
        "Kazakh" => Some(String::from("kk")),
        "Albanian" => Some(String::from("sq")),
        "Swahili" => Some(String::from("sw")),
        "Galician" => Some(String::from("gl")),
        "Marathi" => Some(String::from("mr")),
        "Punjabi" => Some(String::from("pa")),
        "Sinhala" => Some(String::from("si")),
        "Khmer" => Some(String::from("km")),
        "Shona" => Some(String::from("sn")),
        "Yoruba" => Some(String::from("yo")),
        "Somali" => Some(String::from("so")),
        "Afrikaans" => Some(String::from("af")),
        "Occitan" => Some(String::from("oc")),
        "Georgian" => Some(String::from("ka")),
        "Belarusian" => Some(String::from("be")),
        "Tajik" => Some(String::from("tg")),
        "Sindhi" => Some(String::from("sd")),
        "Gujarati" => Some(String::from("gu")),
        "Amharic" => Some(String::from("am")),
        "Yiddish" => Some(String::from("yi")),
        "Lao" => Some(String::from("lo")),
        "Uzbek" => Some(String::from("uz")),
        "Faroese" => Some(String::from("fo")),
        "Haitian creole" => Some(String::from("ht")),
        "Pashto" => Some(String::from("ps")),
        "Turkmen" => Some(String::from("tk")),
        "Nynorsk" => Some(String::from("nn")),
        "Maltese" => Some(String::from("mt")),
        "Sanskrit" => Some(String::from("sa")),
        "Luxembourgish" => Some(String::from("lb")),
        "Myanmar" => Some(String::from("my")),
        "Tibetan" => Some(String::from("bo")),
        "Tagalog" => Some(String::from("tl")),
        "Malagasy" => Some(String::from("mg")),
        "Assamese" => Some(String::from("as")),
        "Tatar" => Some(String::from("tt")),
        "Hawaiian" => Some(String::from("haw")),
        "Lingala" => Some(String::from("ln")),
        "Hausa" => Some(String::from("ha")),
        "Bashkir" => Some(String::from("ba")),
        "Javanese" => Some(String::from("jw")),
        "Sundanese" => Some(String::from("su")),
        "Cantonese" => Some(String::from("yue")),
    };

    pub static ref LANGUAGE_CODES: std::collections::HashMap<Option<String>, &'static str> = maplit::hashmap!{
        None => "Auto",
        Some(String::from("en")) => "English",
        Some(String::from("zh")) => "Mandarin",
        Some(String::from("de")) => "German",
        Some(String::from("es")) => "Spanish",
        Some(String::from("ru")) => "Russian",
        Some(String::from("ko")) => "Korean",
        Some(String::from("fr")) => "French",
        Some(String::from("ja")) => "Japanese",
        Some(String::from("pt")) => "Portuguese",
        Some(String::from("tr")) => "Turkish",
        Some(String::from("pl")) => "Polish",
        Some(String::from("ca")) => "Catalan",
        Some(String::from("nl")) => "Dutch",
        Some(String::from("ar")) => "Arabic",
        Some(String::from("sv")) => "Swedish",
        Some(String::from("it")) => "Italian",
        Some(String::from("id")) => "Indonesian",
        Some(String::from("hi")) => "Hindi",
        Some(String::from("fi")) => "Finnish",
        Some(String::from("vi")) => "Vietnamese",
        Some(String::from("he")) => "Hebrew",
        Some(String::from("uk")) => "Ukrainian",
        Some(String::from("el")) => "Greek",
        Some(String::from("ms")) => "Malay",
        Some(String::from("cs")) => "Czech",
        Some(String::from("ro")) => "Romanian",
        Some(String::from("da")) => "Danish",
        Some(String::from("hu")) => "Hungarian",
        Some(String::from("ta")) => "Tamil",
        Some(String::from("no")) => "Norwegian",
        Some(String::from("th")) => "Thai",
        Some(String::from("ur")) => "Urdu",
        Some(String::from("hr")) => "Croatian",
        Some(String::from("bg")) => "Bulgarian",
        Some(String::from("lt")) => "Lithuanian",
        Some(String::from("la")) => "Latin",
        Some(String::from("mi")) => "Maori",
        Some(String::from("ml")) => "Malayalam",
        Some(String::from("cy")) => "Welsh",
        Some(String::from("sk")) => "Slovak",
        Some(String::from("te")) => "Telugu",
        Some(String::from("fa")) => "Persian",
        Some(String::from("lv")) => "Latvian",
        Some(String::from("bn")) => "Bengali",
        Some(String::from("sr")) => "Serbian",
        Some(String::from("az")) => "Azerbaijani",
        Some(String::from("sl")) => "Slovenian",
        Some(String::from("kn")) => "Kannada",
        Some(String::from("et")) => "Estonian",
        Some(String::from("mk")) => "Macedonian",
        Some(String::from("br")) => "Breton",
        Some(String::from("eu")) => "Basque",
        Some(String::from("is")) => "Icelandic",
        Some(String::from("hy")) => "Armenian",
        Some(String::from("ne")) => "Nepali",
        Some(String::from("mn")) => "Mongolian",
        Some(String::from("bs")) => "Bosnian",
        Some(String::from("kk")) => "Kazakh",
        Some(String::from("sq")) => "Albanian",
        Some(String::from("sw")) => "Swahili",
        Some(String::from("gl")) => "Galician",
        Some(String::from("mr")) => "Marathi",
        Some(String::from("pa")) => "Punjabi",
        Some(String::from("si")) => "Sinhala",
        Some(String::from("km")) => "Khmer",
        Some(String::from("sn")) => "Shona",
        Some(String::from("yo")) => "Yoruba",
        Some(String::from("so")) => "Somali",
        Some(String::from("af")) => "Afrikaans",
        Some(String::from("oc")) => "Occitan",
        Some(String::from("ka")) => "Georgian",
        Some(String::from("be")) => "Belarusian",
        Some(String::from("tg")) => "Tajik",
        Some(String::from("sd")) => "Sindhi",
        Some(String::from("gu")) => "Gujarati",
        Some(String::from("am")) => "Amharic",
        Some(String::from("yi")) => "Yiddish",
        Some(String::from("lo")) => "Lao",
        Some(String::from("uz")) => "Uzbek",
        Some(String::from("fo")) => "Faroese",
        Some(String::from("ht")) => "Haitian creole",
        Some(String::from("ps")) => "Pashto",
        Some(String::from("tk")) => "Turkmen",
        Some(String::from("nn")) => "Nynorsk",
        Some(String::from("mt")) => "Maltese",
        Some(String::from("sa")) => "Sanskrit",
        Some(String::from("lb")) => "Luxembourgish",
        Some(String::from("my")) => "Myanmar",
        Some(String::from("bo")) => "Tibetan",
        Some(String::from("tl")) => "Tagalog",
        Some(String::from("mg")) => "Malagasy",
        Some(String::from("as")) => "Assamese",
        Some(String::from("tt")) => "Tatar",
        Some(String::from("haw")) => "Hawaiian",
        Some(String::from("ln")) => "Lingala",
        Some(String::from("ha")) => "Hausa",
        Some(String::from("ba")) => "Bashkir",
        Some(String::from("jw")) => "Javanese",
        Some(String::from("su")) => "Sundanese",
        Some(String::from("yue")) => "Cantonese",
    };

}
