//! The picker: a ratatui directory browser whose only output is the path to
//! cd into. Renders on stderr so stdout can carry that path back to the
//! calling shell.
//!
//! - [`app`] holds the state and resolves what Enter means.
//! - [`view`] renders it.
//! - [`run`] owns the terminal and the key table.
mod app;
mod run;
mod view;

pub use run::pick;

pub(crate) type Res<T> = Result<T, Box<dyn std::error::Error>>;
