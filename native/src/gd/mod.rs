//! Godot bindings.

pub mod font_asset;
pub mod info;
pub mod mesh;
pub mod text;

use godot::prelude::*;

/// Capability probe: GDScript checks `ClassDB.class_exists("TypesetterNative")`.
#[derive(GodotClass)]
#[class(init, base = RefCounted)]
pub struct TypesetterNative {
    base: Base<RefCounted>,
}

#[godot_api]
impl TypesetterNative {
    #[func]
    fn version() -> GString {
        GString::from(env!("CARGO_PKG_VERSION"))
    }
}
