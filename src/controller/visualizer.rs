use crate::utils::constants::NUM_BUCKETS;
use atomic_enum::atomic_enum;
use crossbeam::thread::{Scope, ScopedJoinHandle};
use parking_lot::RwLock;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use strum::{Display, EnumIter};

// TODO: might need to move this to somewhere shared.
pub(crate) enum RotationDirection {
    Clockwise,
    CounterClockwise,
}
#[atomic_enum]
#[derive(PartialEq, EnumIter, Display)]
pub enum AnalysisType {
    Waveform = 0,
    Power,
    #[strum(to_string = "Spectrum Density")]
    SpectrumDensity,
}

impl AnalysisType {
    // NOTE: this is obviously a little un-maintainable and not the greatest solution if the AnalysisTypes grow.
    // If it becomes untenable, look into a macro-based solution.
    pub(crate) fn rotate(&self, direction: RotationDirection) -> Self {
        match (self, direction) {
            (AnalysisType::Waveform, RotationDirection::Clockwise) => AnalysisType::Power,
            (AnalysisType::Waveform, RotationDirection::CounterClockwise) => {
                AnalysisType::SpectrumDensity
            }
            (AnalysisType::Power, RotationDirection::Clockwise) => AnalysisType::SpectrumDensity,
            (AnalysisType::Power, RotationDirection::CounterClockwise) => AnalysisType::Waveform,
            (AnalysisType::SpectrumDensity, RotationDirection::Clockwise) => AnalysisType::Waveform,
            (AnalysisType::SpectrumDensity, RotationDirection::CounterClockwise) => {
                AnalysisType::Power
            }
        }
    }
}

// TODO: Expect to need to return to this (The underlying FFT utils need significant refactoring)
// TODO: add an internal message queue, finish update_visualizer_data, fix constructor logic (spawn the bg thread)
pub struct VisualizerEngineState {
    buffer: RwLock<[f32; NUM_BUCKETS]>,
    visualizer_running: AtomicBool,
    analysis_type: AtomicAnalysisType,
}
pub struct VisualizerEngine {
    inner: Arc<VisualizerEngineState>,
}

impl VisualizerEngine {
    pub(crate) fn new() -> Self {
        let buffer = RwLock::new([0.0; NUM_BUCKETS]);
        let visualizer_running = AtomicBool::new(false);
        let analysis_type = AtomicAnalysisType::new(AnalysisType::Waveform);
        let inner = Arc::new(VisualizerEngineState {
            buffer,
            visualizer_running,
            analysis_type,
        });
        Self { inner }
    }


    pub(crate) fn set_visualizer_visibility(&self, visibility: bool) {
        self.inner
            .visualizer_running
            .store(visibility, Ordering::Release);
    }

    pub(crate) fn visualizer_running(&self) -> bool {
        self.inner.visualizer_running.load(Ordering::Acquire)
    }

    // TODO: get rid of this function, move the logic to a spawned thread that runs in the background.
    pub(crate) fn run_scoped_visualizer_analysis<'scope>(scope: &'scope Scope<'scope>) -> ScopedJoinHandle<'scope, ()> {
        todo!("Migrate the logic over here from the TranscriberLoop")
    }

    pub(crate) fn update_visualizer_data(&self, buffer: &[f32]) {
        // TODO: If the public method gets removed, just make the atomic load here.
        if self.visualizer_running() {
            todo!("Write into an internal message queueue")
        }
    }

    // TODO: this method can probably be removed--it's a one-line, likely one-use thing that can happen a little closer to the hot loop
    fn update_visualization_buffer(&self, buffer: &[f32; NUM_BUCKETS]) {
        self.inner.buffer.write().copy_from_slice(buffer)
    }
    pub(crate) fn try_read_visualization_buffer(&self, copy_buffer: &mut [f32; NUM_BUCKETS]) {
        if let Some(buffer) = self.inner.buffer.try_read() {
            copy_buffer.copy_from_slice(buffer.deref())
        }
    }
    pub(crate) fn get_visualizer_analysis_type(&self) -> AnalysisType {
        self.inner.analysis_type.load(Ordering::Acquire)
    }

    // There's no real contention here; rotations are rare,
    // and this isn't RMW critical, so this can be load -> rotate -> store.
    pub(crate) fn rotate_visualizer_type(&self, direction: RotationDirection) {
        self.inner.analysis_type.store(
            self.inner
                .analysis_type
                .load(Ordering::Acquire)
                .rotate(direction),
            Ordering::Release,
        )
    }
}

// TODO: implement drop and implement RAII-style background thread
