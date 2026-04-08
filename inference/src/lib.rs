//! Claw Pen Inference Service
//!
//! A native Rust inference service for running quantized LLMs locally.

pub mod model;
pub mod inference;
pub mod api;

#[cfg(test)]
mod tests;

pub use model::{ModelLoader, ModelConfig, SamplingParams, GenerateRequest, GenerateResponse};
pub use inference::InferenceEngine;
pub use api::InferenceApi;
