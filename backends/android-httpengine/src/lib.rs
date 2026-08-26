//! <style>
//! .rustdoc-hidden { display: none; }
//! </style>

#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(all(target_os = "android", feature = "async"))]
mod r#async;
#[cfg(target_os = "android")]
mod backend;
#[cfg(target_os = "android")]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/android_bindings.rs"));
}
#[cfg(all(target_os = "android", feature = "blocking"))]
mod blocking;
#[cfg(target_os = "android")]
mod callback;
#[cfg(target_os = "android")]
mod error;
#[cfg(target_os = "android")]
mod request;
#[cfg(target_os = "android")]
mod response;
#[cfg(target_os = "android")]
mod state;

#[cfg(target_os = "android")]
use backend::AndroidBackend;
#[cfg(target_os = "android")]
use jni::{objects::JObject, Env};

/// The backend implementation using an application-provided `android.net.http.HttpEngine`.
#[derive(Clone)]
pub struct HttpEngineBackend {
    #[cfg(target_os = "android")]
    inner: AndroidBackend,
}

#[cfg(target_os = "android")]
impl HttpEngineBackend {
    /// Creates a backend using an existing `android.net.http.HttpEngine` and callback executor.
    ///
    /// `HttpEngine` is available on Android API 34 or Android S extension 7. The application owns
    /// availability checks and engine configuration before constructing this backend.
    pub fn new(
        env: &mut Env<'_>,
        engine: &JObject<'_>,
        executor: &JObject<'_>,
    ) -> jni::errors::Result<Self> {
        Ok(Self {
            inner: AndroidBackend::new(env, engine, executor)?,
        })
    }
}

#[cfg(target_os = "android")]
/// Registers an initialized [`HttpEngineBackend`] as the global default.
pub fn register(backend: HttpEngineBackend) {
    nyquest_interface::register_backend(backend);
}

#[cfg(all(target_os = "android", feature = "async"))]
impl nyquest_interface::r#async::AsyncBackend for HttpEngineBackend {
    type AsyncClient = r#async::AndroidAsyncClient;

    async fn create_async_client(
        &self,
        options: nyquest_interface::client::ClientOptions,
    ) -> nyquest_interface::Result<Self::AsyncClient> {
        self.inner.create_async_client(options)
    }
}

#[cfg(all(target_os = "android", feature = "blocking"))]
impl nyquest_interface::blocking::BlockingBackend for HttpEngineBackend {
    type BlockingClient = blocking::AndroidBlockingClient;

    fn create_blocking_client(
        &self,
        options: nyquest_interface::client::ClientOptions,
    ) -> nyquest_interface::Result<Self::BlockingClient> {
        self.inner.create_blocking_client(options)
    }
}
