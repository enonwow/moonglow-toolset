//! The build script generates include_str! entries, never a runtime repository lookup.
include!(concat!(env!("OUT_DIR"), "/live_sources.rs"));
