//! Version-matched native Java item resource selection and baking.
#![forbid(unsafe_code)]
pub mod bake;
pub mod item_bake;
pub mod item_model;
mod json;
pub mod math;
pub mod model;
pub mod obj;
pub mod resolve;
pub use json::DecodeError;
