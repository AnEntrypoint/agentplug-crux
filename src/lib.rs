pub mod baseline;
pub mod dedup;
pub mod durations;
pub mod emit;
pub mod event;
pub mod normalize;
pub mod score;
pub mod skiplist;

#[cfg(not(target_arch = "wasm32"))]
pub mod native_ingest;

#[cfg(target_arch = "wasm32")]
pub mod abi;
#[cfg(target_arch = "wasm32")]
pub mod pipeline;
#[cfg(target_arch = "wasm32")]
pub mod wasm_ingest;

#[cfg(target_arch = "wasm32")]
pub use abi::{plugin_call, plugkit_alloc, plugkit_free};
