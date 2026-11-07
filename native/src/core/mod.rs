//! Engine-independent core: everything here is plain Rust with no Godot types,
//! so it can be unit-tested and benchmarked with `cargo test`.

pub mod atlas;
pub mod font;
pub mod layout;
pub mod richtext;
pub mod msdf;
pub mod outline;
pub mod pack;
pub mod raster;
pub mod sdf;
