use std::time::Duration;
use std::{fmt, sync::Arc};

#[cfg(feature = "blocking")]
use nyquest_interface::blocking::AnyBlockingBackend;
use nyquest_interface::client::{CachingBehavior, ClientOptions, ProxyOptions};
#[cfg(feature = "async")]
use nyquest_interface::r#async::AnyAsyncBackend;

#[cfg(doc)]
use crate::client::CustomProxy;

/// A builder for creating an async or blocking client with custom options.
///
/// Use [`ClientBuilder::default()`] to create a new builder instance.
#[derive(Clone, Default)]
pub struct ClientBuilder {
    pub(crate) options: ClientOptions,
    #[cfg(feature = "async")]
    pub(crate) async_backend: Option<Arc<dyn AnyAsyncBackend>>,
    #[cfg(feature = "blocking")]
    pub(crate) blocking_backend: Option<Arc<dyn AnyBlockingBackend>>,
}

impl fmt::Debug for ClientBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = f.debug_struct("ClientBuilder");
        builder.field("options", &self.options);
        #[cfg(feature = "async")]
        builder.field("has_async_backend", &self.async_backend.is_some());
        #[cfg(feature = "blocking")]
        builder.field("has_blocking_backend", &self.blocking_backend.is_some());
        builder.finish()
    }
}

impl ClientBuilder {
    /// Uses `backend` for the async client built by this builder.
    ///
    /// A client-specific backend takes precedence over the globally registered backend. This is
    /// useful for libraries that accept a configured HTTP client, applications that need more than
    /// one backend, and tests that inject a local backend. Other builders and existing clients are
    /// not affected.
    #[cfg(feature = "async")]
    #[cfg_attr(docsrs, doc(cfg(feature = "async")))]
    pub fn async_backend(mut self, backend: impl AnyAsyncBackend) -> Self {
        self.async_backend = Some(Arc::new(backend));
        self
    }

    /// Uses `backend` for the blocking client built by this builder.
    ///
    /// A client-specific backend takes precedence over the globally registered backend. This is
    /// useful for libraries that accept a configured HTTP client, applications that need more than
    /// one backend, and tests that inject a local backend. Other builders and existing clients are
    /// not affected.
    #[cfg(feature = "blocking")]
    #[cfg_attr(docsrs, doc(cfg(feature = "blocking")))]
    pub fn blocking_backend(mut self, backend: impl AnyBlockingBackend) -> Self {
        self.blocking_backend = Some(Arc::new(backend));
        self
    }

    /// Sets the base URL for the client.
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.options.base_url = Some(base_url.into());
        self
    }

    /// Sets the `user-agent` header for the client.
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.options.user_agent = Some(user_agent.into());
        self
    }

    /// Adds a request header to all requests made with this client.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.options
            .default_headers
            .push((name.into(), value.into()));
        self
    }

    /// Instructs the backend to bypass cache.
    #[inline]
    pub fn no_caching(mut self) -> Self {
        self.options.caching_behavior = CachingBehavior::Disabled;
        self
    }

    /// Instructs the backend to bypass preset proxies.
    ///
    /// This overrides the [`Self::custom_proxy`] setting.
    #[inline]
    pub fn no_proxy(mut self) -> Self {
        self.options.proxy_options = ProxyOptions::None;
        self
    }

    /// Sets custom proxy settings from a [`CustomProxy`] configuration.
    ///
    /// This overrides the [`Self::no_proxy`] setting.
    ///
    /// # Note
    ///
    /// The backend may ignore the custom proxy settings if the underlying implementation does not
    /// support them (e.g., WinRT backend) or if the proxy configuration is invalid.
    #[inline]
    pub fn custom_proxy(mut self, proxy: impl super::proxy::IntoProxyOptions) -> Self {
        self.options.proxy_options = proxy.into_proxy_options();
        self
    }

    /// Instructs the backend to not keep cookies between requests.
    #[inline]
    pub fn no_cookies(mut self) -> Self {
        self.options.use_cookies = false;
        self
    }

    /// Instructs the backend to not follow 3xx redirects.
    #[inline]
    pub fn no_redirects(mut self) -> Self {
        self.options.follow_redirects = false;
        self
    }

    /// Sets the maximum number of bytes to buffer for a response.
    ///
    /// # Note
    ///
    /// The limit only applies to `response.bytes()` and `response.text()`.
    /// Streaming is not affected.
    #[inline]
    pub fn max_response_buffer_size(mut self, size: u64) -> Self {
        self.options.max_response_buffer_size = Some(size);
        self
    }

    /// Sets the timeout for a whole request to complete.
    ///
    /// # Note
    ///
    /// The precision of the timeout is implementation defined.
    #[inline]
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.options.request_timeout = Some(timeout);
        self
    }

    /// Allows the client to ignore certificate errors.
    ///
    /// # Warning
    ///
    /// SSL server certificate errors should only be ignored in advanced scenarios. Disregarding
    /// server certificate errors may result in the loss of privacy or integrity of the content
    /// passed over the SSL session.
    #[inline]
    pub fn dangerously_ignore_certificate_errors(mut self) -> Self {
        self.options.ignore_certificate_errors = true;
        self
    }
}
