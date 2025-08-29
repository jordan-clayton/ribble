use ribble_whisper::whisper::configs::RealtimeBufferingStrategy;
use strum::{AsRefStr, EnumIter, EnumString, IntoStaticStr};
// Adapter-ish enum to simplify the buffering strategy complexity for real-time streaming.

#[derive(Copy, Clone, Default, Eq, PartialEq, EnumIter, IntoStaticStr, AsRefStr, EnumString)]
pub(crate) enum RibbleBufferingStrategy {
    #[default]
    Continuous,
    #[strum(serialize = "Buffered: S")]
    ShortBuffered,
    #[strum(serialize = "Buffered: L")]
    LongBuffered,
}

impl RibbleBufferingStrategy {
    pub(crate) fn tooltip(&self) -> &str {
        match self {
            RibbleBufferingStrategy::Continuous => "No buffering.",
            RibbleBufferingStrategy::ShortBuffered => "Short buffering: 3s",
            RibbleBufferingStrategy::LongBuffered => "Long buffering: 6s",
        }
    }
}

const CONTINUOUS_MS: usize = 1000;
const SHORT_BUFFER_MS: usize = 3000;
const LONG_BUFFER_MS: usize = 6000;

impl From<RealtimeBufferingStrategy> for RibbleBufferingStrategy {
    fn from(value: RealtimeBufferingStrategy) -> Self {
        match value {
            RealtimeBufferingStrategy::Buffered { buffer_ms }
            if buffer_ms > CONTINUOUS_MS && buffer_ms < LONG_BUFFER_MS =>
                {
                    RibbleBufferingStrategy::ShortBuffered
                }
            RealtimeBufferingStrategy::Buffered { buffer_ms } if buffer_ms >= LONG_BUFFER_MS => {
                RibbleBufferingStrategy::LongBuffered
            }

            _ => RibbleBufferingStrategy::Continuous,
        }
    }
}

impl From<RibbleBufferingStrategy> for RealtimeBufferingStrategy {
    fn from(value: RibbleBufferingStrategy) -> Self {
        match value {
            RibbleBufferingStrategy::Continuous => RealtimeBufferingStrategy::Continuous,
            RibbleBufferingStrategy::ShortBuffered => RealtimeBufferingStrategy::Buffered {
                buffer_ms: SHORT_BUFFER_MS,
            },
            RibbleBufferingStrategy::LongBuffered => RealtimeBufferingStrategy::Buffered {
                buffer_ms: LONG_BUFFER_MS,
            },
        }
    }
}
