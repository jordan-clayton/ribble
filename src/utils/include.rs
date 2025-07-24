use crate::utils::errors::RibbleError;
use std::path::Path;

// TODO: this will need to be corrected once this is figured out
const DEFAULT_TINY_MODEL = include_bytes!("ggml-tiny.q0.bin");
// TODO: this will need to be corrected once this is figured out
const DEFAULT_SMALL_MODEL = include_bytes!("ggml-small.q0.bin");
// TODO: Pick a "larger" model that isn't too big to include for more accurate transcription
const DEFAULT_LARGE_MODEL = include_bytes!("ggml-large.q0.bin");


pub(crate) fn confirm_models_copied(_path: &Path) -> bool {
    todo!("Finish the models copied test.");
}

pub(crate) fn copy_model_includes(_path: &Path) -> Result<(), RibbleError> {
    todo!("Finish the copy routine");
    // For each of the included default models: 
    // Open up a file for writing binary (use a named constant for the filename)
    // Flush the bytes out.
    // return Ok(())
}
