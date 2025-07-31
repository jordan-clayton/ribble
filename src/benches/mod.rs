use crate::controller::NUM_VISUALIZER_BUCKETS;

#[cfg(all(feature = "bencher", test))]
pub(crate) mod visualizer_pane_benchmark;

pub(crate) trait VisualizerPaneTester {
    fn get_buckets(&mut self) -> &mut [f32; NUM_VISUALIZER_BUCKETS];
    fn smoothing(&mut self, dt: f32);
}
