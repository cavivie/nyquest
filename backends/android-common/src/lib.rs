//! Shared implementation details for Nyquest backends using Android Java HTTP APIs.

#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(target_os = "android")]
mod android;

#[cfg(target_os = "android")]
pub use android::{AndroidAsyncClient, AndroidBackend, AndroidBlockingClient, BackendBindings};
