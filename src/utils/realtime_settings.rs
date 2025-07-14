use std::time::Duration;
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoStaticStr};

// NOTE: 300ms generally seems to be okay for VAD, but users might see better results from
// a slightly larger buffer (~1sec).
//
// For now, expose a limited selection of predefined time lengths to allow a small amount
// of configuration.

const RT300MSEC: usize = 300;
const RT500MSEC: usize = 500;

const RT1SEC: usize = 1000;

const RT3SEC: usize = 3000;
const RT5SEC: usize = 5000;
const RT10SEC: usize = 10_000;
const RT20SEC: usize = 20_000;

const RT15MIN: u128 = Duration::from_secs(15 * 60).as_millis();
const RT30MIN: u128 = Duration::from_secs(30 * 60).as_millis();
const RT1HR: u128 = Duration::from_secs(60 * 60).as_millis();
const RT2HR: u128 = Duration::from_secs(2 * 60).as_millis();
const RTINF: u128 = 0;

// NOTE: the From<_> implementations may not be the most logically sound.
// However, to limit the granularity of settings and to reduce the amount of
// typing (& excessive Traits that achieve the same thing), these members
// implement From to map user-facing selections to internal settings.

// NOTE: If it's worth it to include an explicit "Auto", then remove Ord/PartialOrd
#[derive(
    Copy,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    EnumString,
    Display,
    EnumIter,
    AsRefStr,
    IntoStaticStr,
)]
pub(crate) enum RealtimeTimeout {
    #[strum(serialize = "15 Min")]
    Rt15Min,
    #[strum(serialize = "30 Min")]
    Rt30Min,
    #[default]
    #[strum(serialize = "1 Hr")]
    Rt1Hr,
    #[strum(serialize = "2 Hr")]
    Rt2Hr,
    #[strum(serialize = "None")]
    Infinite,
}

impl From<RealtimeTimeout> for u128 {
    fn from(value: RealtimeTimeout) -> Self {
        match value {
            RealtimeTimeout::Rt15Min => RT15MIN,
            RealtimeTimeout::Rt30Min => RT30MIN,
            RealtimeTimeout::Rt1Hr => RT1HR,
            RealtimeTimeout::Rt2Hr => RT2HR,
            // Ribble_Whisper uses 0 for "infinite"
            RealtimeTimeout::Infinite => RTINF,
        }
    }
}

// If the mapping somehow isn't 1:1 within the app, this will just fallback to defaults.
impl From<u128> for RealtimeTimeout {
    fn from(value: u128) -> Self {
        match value {
            RT15MIN => RealtimeTimeout::Rt15Min,
            RT30MIN => RealtimeTimeout::Rt30Min,
            RT1HR => RealtimeTimeout::Rt1Hr,
            RT2HR => RealtimeTimeout::Rt2Hr,
            RTINF => RealtimeTimeout::Infinite,
            _ => RealtimeTimeout::default(),
        }
    }
}

// NOTE: it's a little tricky with the From<_> conversions to add an "Auto" as a fallback
// to match the default. (e.g. usize -> Auto or Large).
// Implementing that manually involves extra state management that comes with very little
// benefit; it should be sufficient to just have a "default" member that maps 1:1 with
// ribble_whisper defaults.
#[derive(
    Copy,
    Clone,
    Default,
    PartialEq,
    Eq,
    Ord,
    PartialOrd,
    EnumString,
    Display,
    EnumIter,
    AsRefStr,
    IntoStaticStr,
)]
pub(crate) enum AudioSampleLen {
    // 3s
    Small,
    // 5s
    Medium,
    // 10s
    #[default]
    Large,
    // 20s
    Largest,
}

impl From<AudioSampleLen> for usize {
    fn from(value: AudioSampleLen) -> Self {
        match value {
            AudioSampleLen::Small => RT3SEC,
            AudioSampleLen::Medium => RT5SEC,
            AudioSampleLen::Large => RT10SEC,
            AudioSampleLen::Largest => RT20SEC,
        }
    }
}

impl From<usize> for AudioSampleLen {
    fn from(value: usize) -> Self {
        match value {
            RT3SEC => AudioSampleLen::Small,
            RT5SEC => AudioSampleLen::Medium,
            RT10SEC => AudioSampleLen::Large,
            RT20SEC => AudioSampleLen::Largest,
            // Invalid values just fall back to 10s (default).
            _ => AudioSampleLen::default(),
        }
    }
}

#[derive(
    Copy,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    EnumString,
    Display,
    EnumIter,
    AsRefStr,
    IntoStaticStr,
)]
pub(crate) enum VadSampleLen {
    #[default]
    Small,
    Medium,
    Large,
}

impl From<VadSampleLen> for usize {
    fn from(value: VadSampleLen) -> Self {
        match value {
            VadSampleLen::Small => RT300MSEC,
            VadSampleLen::Medium => RT500MSEC,
            VadSampleLen::Large => RT1SEC,
        }
    }
}

impl From<usize> for VadSampleLen {
    fn from(value: usize) -> Self {
        match value {
            RT300MSEC => VadSampleLen::Small,
            RT500MSEC => VadSampleLen::Medium,
            RT1SEC => VadSampleLen::Large,
            _ => VadSampleLen::default(),
        }
    }
}
