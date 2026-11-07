//! Platform-independent TinyHand logic.
//!
//! Everything here is pure Rust with no OS calls, so it can drive the Windows
//! overlay today and a macOS or Linux overlay later. A platform layer feeds the
//! [`Engine`] cursor positions and button events, and copies the pixels it
//! renders onto a transparent, click-through window.

pub mod arm;
pub mod asset;
pub mod catalog;
pub mod engine;
pub mod geom;
pub mod ink;
pub mod interact;
pub mod prefs;
mod raster;
pub mod tools;

pub use asset::HandAsset;
pub use engine::{Engine, Look, Settings};
pub use geom::IRect;
pub use prefs::Prefs;
