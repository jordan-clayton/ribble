pub(crate) mod app;
mod widgets;

#[cfg(not(feature = "bencher"))]
mod panes;
#[cfg(feature = "bencher")]
pub(crate) mod panes;

