use crate::utils::progress::Progress;
use ribble_whisper::whisper::configs::{WhisperConfigsV2, WhisperRealtimeConfigs};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use parking_lot::Mutex;
use crate::controller::{RibbleWorkerHandle, UIThreadOnly};

type ProgressCallback = Box<dyn Fn(Progress) + Send + Sync>;

struct TranscriberState{
    // TODO: file pathing for offline transcription -> or just accept as an argument to the function.
    // Not sure whether it can/should stay in the UI or not.
    realtime_configs: UIThreadOnly<WhisperRealtimeConfigs>,
    offline_configs: UIThreadOnly<WhisperConfigsV2>,
    realtime_running: AtomicBool,
    offline_running: AtomicBool,
    on_progress: Mutex<Option<ProgressCallback>>,
}

struct TranscriberEngine {
    inner: Arc<TranscriberState>,
}

impl TranscriberEngine {
    pub(crate) fn new(
        realtime_configs: WhisperRealtimeConfigs,
        offline_configs: WhisperConfigsV2,
    ) -> Self {
        
        let realtime_running = AtomicBool::new(false);
        let offline_running = AtomicBool::new(false);
        let realtime_configs = UIThreadOnly::new(realtime_configs);
        let offline_configs = UIThreadOnly::new(offline_configs);
        
        let inner = Arc::new(TranscriberState{
            realtime_configs,
            offline_configs,
            realtime_running,
            offline_running,
            on_progress: Mutex::new(None),
        });
        Self{inner}
    }
    
    // TODO: remove if unused.
    pub(crate) fn transcriber_running(&self) -> bool {
        self.realtime_running()
            || self.offline_running()
    }
    pub(crate) fn realtime_running(&self) -> bool{
        self.inner.realtime_running.load(Ordering::Acquire)
    }
    pub(crate) fn offline_running(&self) -> bool {
        self.inner.offline_running.load(Ordering::Acquire)
    }
    pub(crate) fn set_on_progress(&self, callback: Option<impl Fn(Progress) + Send + Sync + 'static>){
        let lock = self.inner.on_progress.lock();
        *lock = callback.map(|cb| Box::new(cb) as Box<_>); 
    }
   
    // NOTE TO SELF: remember to dereference the binding when calling builder methods to mutate,
    // otherwise, it'll just change the local binding.
    pub(crate) unsafe fn realtime_configs(&self) -> &mut WhisperRealtimeConfigs {
        self.inner.realtime_configs.get_mut()
    }
    pub(crate) unsafe fn offline_configs(&self) -> &mut WhisperConfigsV2 {
        self.inner.offline_configs.get_mut()
    }
    pub(crate) fn run_realtime(&self) -> RibbleWorkerHandle{
        todo!();
    }
    
    // TODO: determine whether or not to take the path here as an argument, or whether to hold state
    pub(crate) fn run_offline(&self) -> RibbleWorkerHandle{
        todo!();
    }
}
