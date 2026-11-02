//! `podbox publish`: turn repo definitions into prebuilt OCI images.
//!
//! Pure core (manifest parsing, discovery, naming, tag expansion, plan,
//! build-key, labels) compiles without the `cli` feature so the wasm
//! engine can share it. I/O (registry probes, engine invocation, event
//! rendering) is behind `#[cfg(feature = "cli")]`.

pub mod build_key;
pub mod context;
pub mod discover;
pub mod engine;
pub mod events;
pub mod labels;
pub mod manifest;
pub mod naming;
pub mod plan;
pub mod registry;
pub mod tags;

#[cfg(feature = "cli")]
pub mod commands;
