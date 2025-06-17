// TODO: RecorderEngine -> handle recording stuff here
// NOTE: use the kernel to spawn the write thread, only run the audio fanout in the recording loop
// API needs:
// -> (Possibly) an exposed output file handle
// -> constructors
// -> kernel setter
// -> Accessors (read/write locks) for Configs
// -> The recording loop
pub struct RecorderEngine {}