pub(crate) mod app;
pub(crate) mod tabs;
pub(crate) mod widgets;

// TODO: rename this to tabs once the rewrite is finished.
pub(crate) mod new_tabs;

pub(crate) enum TranscriptionType {
    Realtime,
    Offline,
}
