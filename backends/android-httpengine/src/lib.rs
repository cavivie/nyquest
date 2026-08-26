//! <style>
//! .rustdoc-hidden { display: none; }
//! </style>

#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(target_os = "android")]
mod android;

#[cfg(target_os = "android")]
use android::{AndroidBackend, BackendBindings};
#[cfg(target_os = "android")]
use jni::{objects::JObject, JNIEnv};

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
        env: &mut JNIEnv<'_>,
        engine: &JObject<'_>,
        executor: &JObject<'_>,
    ) -> jni::errors::Result<Self> {
        Ok(Self {
            inner: AndroidBackend::new(env, engine, executor, HTTP_ENGINE_BINDINGS)?,
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
    type AsyncClient = android::AndroidAsyncClient;

    async fn create_async_client(
        &self,
        options: nyquest_interface::client::ClientOptions,
    ) -> nyquest_interface::Result<Self::AsyncClient> {
        self.inner.create_async_client(options)
    }
}

#[cfg(all(target_os = "android", feature = "blocking"))]
impl nyquest_interface::blocking::BlockingBackend for HttpEngineBackend {
    type BlockingClient = android::AndroidBlockingClient;

    fn create_blocking_client(
        &self,
        options: nyquest_interface::client::ClientOptions,
    ) -> nyquest_interface::Result<Self::BlockingClient> {
        self.inner.create_blocking_client(options)
    }
}

#[cfg(target_os = "android")]
const HTTP_ENGINE_BINDINGS: BackendBindings = BackendBindings {
    name: "AndroidHttpEngine",
    callback_class: "io/nyquest/httpengine/NativeUrlRequestCallback",
    callback_constructor: "(J)V",
    engine_builder_signature: "(Ljava/lang/String;Ljava/util/concurrent/Executor;Landroid/net/http/UrlRequest$Callback;)Landroid/net/http/UrlRequest$Builder;",
    callback_before_executor: false,
    builder_class: "android/net/http/UrlRequest$Builder",
    request_class: "android/net/http/UrlRequest",
    upload_provider_class: "io/nyquest/httpengine/NativeUrlRequestCallback$ByteArrayUploadProvider",
    upload_provider_signature: "Landroid/net/http/UploadDataProvider;",
    disable_cache_method: "setCacheDisabled",
    disable_cache_signature: "(Z)Landroid/net/http/UrlRequest$Builder;",
    disable_cache_takes_boolean: true,
};
