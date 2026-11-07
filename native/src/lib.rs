//! Typesetter native core. `core` is engine-independent Rust; `gd` binds it to
//! Godot through gdext.

pub mod core;
mod gd;

use godot::prelude::*;

struct TypesetterExtension;

#[gdextension]
unsafe impl ExtensionLibrary for TypesetterExtension {}
