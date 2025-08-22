use core::f32;
// This maybe doesn't need to be a struct, but it is a way to reason about the bounds and not have
// to worry about the application.
//
// Maybe this should be a 30 db reference; not sure?
use std::borrow::Borrow;

// This is in decibels
pub const MAX_AUDIO_GAIN_DB: f32 = 20.0;
// Decibel => max multiplier is 10
const DECIBEL: f32 = 10.0;

#[derive(Default, Copy, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct AudioGainConfigs {
    db: f32,
    use_offline: bool,
}

impl AudioGainConfigs {
    pub(crate) fn new() -> Self {
        Default::default()
    }

    pub(crate) fn with_decibels(mut self, db: f32) -> Self {
        self.db = db.clamp(0.0, MAX_AUDIO_GAIN_DB);
        self
    }

    pub(crate) fn with_use_offline(mut self, use_offline: bool) -> Self {
        self.use_offline = use_offline;
        self
    }

    pub(crate) fn db(&self) -> f32 {
        self.db
    }

    pub(crate) fn use_offline(&self) -> bool {
        self.use_offline
    }

    pub(crate) fn no_gain(&self) -> bool {
        (self.db - 0.0) <= f32::EPSILON
    }

    pub(crate) fn build_audio_gain(&self) -> AudioGain {
        AudioGain::from_db(self.db)
    }
}

#[derive(Copy, Clone)]
pub(crate) struct AudioGain {
    decibel: f32,
    multiplier: f32,
}

impl AudioGain {
    pub(crate) fn new() -> Self {
        Self {
            decibel: 0.0,
            multiplier: 1.0,
        }
    }
    pub(crate) fn from_db(decibel: f32) -> Self {
        let db = decibel.clamp(0.0, MAX_AUDIO_GAIN_DB);
        // TODO: Detemrine whether to clamp to a range or just the min for nonzero mul
        let mul = Self::db_to_mul(db).clamp(1.0, DECIBEL);
        assert!(mul.is_finite());
        Self {
            decibel: db,
            multiplier: mul,
        }
    }
    pub(crate) fn from_mul(mul: f32) -> Self {
        let mul = mul.clamp(1.0, 10.0);
        let db = Self::mul_to_db(mul);
        assert!(db.is_finite());
        Self {
            decibel: db,
            multiplier: mul,
        }
    }

    pub(crate) fn set_db(&mut self, db: f32) {
        let new_db = db.clamp(0.0, MAX_AUDIO_GAIN_DB);
        let new_mul = Self::db_to_mul(new_db);
        assert!(new_mul.is_finite());
        self.decibel = new_db;
        self.multiplier = new_mul;
    }

    pub(crate) fn db(&self) -> f32 {
        self.decibel
    }

    pub(crate) fn mul(&self) -> f32 {
        self.multiplier
    }

    // Either the multiplier or the db value can be used
    // The multiplier is mainly just there to precompute before the iterator.
    pub(crate) fn no_gain(&self) -> bool {
        (self.decibel - 0.0) <= f32::EPSILON
    }

    pub(crate) fn apply_gain<'a, I>(&self, iter: I)
    where
        I: Iterator<Item=&'a mut f32>,
    {
        iter.for_each(|f| *f = (*f * self.multiplier).clamp(-DECIBEL, DECIBEL))
    }

    pub(crate) fn apply_gain_map<I>(self, signal: I) -> AudioGainMap<I>
    where
        I: Iterator,
    {
        AudioGainMap {
            gain: self,
            map_iter: signal,
        }
    }

    fn db_to_mul(db: f32) -> f32 {
        DECIBEL.powf(db / MAX_AUDIO_GAIN_DB)
    }

    // I'm not sure if this will get optimized/whether base 10 has any optimizations
    // If using a different base than Decibel, this will need to remain; otherwise, swap it for
    // log10
    fn mul_to_db(mul: f32) -> f32 {
        MAX_AUDIO_GAIN_DB * mul.log(DECIBEL)
    }
}

impl Default for AudioGain {
    fn default() -> Self {
        Self::new()
    }
}

// NOTE: it's highly unlikely for there to be more than two similar implementations (re: DCBlockMap), but if that's
// the case, then perhaps it is wise to create a generic factory.
pub(crate) struct AudioGainMap<I> {
    gain: AudioGain,
    map_iter: I,
}

impl<I> Iterator for AudioGainMap<I>
where
    I: Iterator,
    I::Item: Borrow<f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        self.map_iter
            .next().map(|f| (*f.borrow() * self.gain.mul()).clamp(-DECIBEL, DECIBEL))
    }
}
