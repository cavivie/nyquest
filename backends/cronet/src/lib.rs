//! <style>
//! .rustdoc-hidden { display: none; }
//! </style>

#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(all(target_os = "android", feature = "async"))]
mod r#async;
#[cfg(target_os = "android")]
mod backend;
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
use backend::{AndroidBackend, BackendBindings};
#[cfg(target_os = "android")]
use jni::{objects::JObject, JNIEnv};

/// The backend implementation using an application-provided Cronet engine.
#[derive(Clone)]
pub struct CronetBackend {
    #[cfg(target_os = "android")]
    inner: AndroidBackend,
}

#[cfg(target_os = "android")]
impl CronetBackend {
    /// Creates a backend using an existing `org.chromium.net.CronetEngine` and callback executor.
    ///
    /// The application owns provider selection and engine configuration. Both Java objects are
    /// retained as global JNI references for the lifetime of this backend.
    pub fn new(
        env: &mut JNIEnv<'_>,
        engine: &JObject<'_>,
        executor: &JObject<'_>,
    ) -> jni::errors::Result<Self> {
        Ok(Self {
            inner: AndroidBackend::new(env, engine, executor, CRONET_BINDINGS)?,
        })
    }
}

#[cfg(target_os = "android")]
/// Registers an initialized [`CronetBackend`] as the global default.
pub fn register(backend: CronetBackend) {
    nyquest_interface::register_backend(backend);
}

#[cfg(all(target_os = "android", feature = "async"))]
impl nyquest_interface::r#async::AsyncBackend for CronetBackend {
    type AsyncClient = r#async::AndroidAsyncClient;

    async fn create_async_client(
        &self,
        options: nyquest_interface::client::ClientOptions,
    ) -> nyquest_interface::Result<Self::AsyncClient> {
        self.inner.create_async_client(options)
    }
}

#[cfg(all(target_os = "android", feature = "blocking"))]
impl nyquest_interface::blocking::BlockingBackend for CronetBackend {
    type BlockingClient = blocking::AndroidBlockingClient;

    fn create_blocking_client(
        &self,
        options: nyquest_interface::client::ClientOptions,
    ) -> nyquest_interface::Result<Self::BlockingClient> {
        self.inner.create_blocking_client(options)
    }
}

#[cfg(target_os = "android")]
const CRONET_BINDINGS: BackendBindings = BackendBindings {
    name: "Cronet",
    callback_class: "io/nyquest/cronet/NativeUrlRequestCallback",
    callback_constructor: "(J)V",
    engine_builder_signature: "(Ljava/lang/String;Lorg/chromium/net/UrlRequest$Callback;Ljava/util/concurrent/Executor;)Lorg/chromium/net/UrlRequest$Builder;",
    callback_before_executor: true,
    builder_class: "org/chromium/net/UrlRequest$Builder",
    request_class: "org/chromium/net/UrlRequest",
    upload_provider_class: "io/nyquest/cronet/NativeUrlRequestCallback$ByteArrayUploadProvider",
    upload_provider_signature: "Lorg/chromium/net/UploadDataProvider;",
    disable_cache_method: "disableCache",
    disable_cache_signature: "()Lorg/chromium/net/UrlRequest$Builder;",
    disable_cache_takes_boolean: false,
};
