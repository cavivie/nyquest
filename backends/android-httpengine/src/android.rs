use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::fmt;
use std::io;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::task::Waker;
use std::thread;
use std::time::Duration;

#[cfg(feature = "async")]
use std::future::poll_fn;
#[cfg(feature = "async")]
use std::pin::Pin;
#[cfg(feature = "async")]
use std::task::Poll;

use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JObjectArray, JString, JValue};
use jni::sys::{jboolean, jint, jlong, JNI_FALSE, JNI_TRUE};
use jni::{JNIEnv, JavaVM, NativeMethod};
use nyquest_interface::client::{CachingBehavior, ClientOptions};
use nyquest_interface::{Error, Method, Result};

/// Java API details needed by the shared Android request driver.
#[derive(Clone, Copy)]
pub struct BackendBindings {
    pub name: &'static str,
    pub callback_class: &'static str,
    pub callback_constructor: &'static str,
    pub engine_builder_signature: &'static str,
    pub callback_before_executor: bool,
    pub builder_class: &'static str,
    pub request_class: &'static str,
    pub upload_provider_class: &'static str,
    pub upload_provider_signature: &'static str,
    pub disable_cache_method: &'static str,
    pub disable_cache_signature: &'static str,
    pub disable_cache_takes_boolean: bool,
}

#[derive(Clone)]
pub struct AndroidBackend {
    core: Arc<BackendCore>,
}

struct BackendCore {
    vm: Arc<JavaVM>,
    engine: GlobalRef,
    executor: GlobalRef,
    callback_class: GlobalRef,
    upload_provider_class: GlobalRef,
    bindings: BackendBindings,
}

impl AndroidBackend {
    pub fn new(
        env: &mut JNIEnv<'_>,
        engine: &JObject<'_>,
        executor: &JObject<'_>,
        bindings: BackendBindings,
    ) -> jni::errors::Result<Self> {
        let callback_class = env.find_class(bindings.callback_class)?;
        register_callback_natives(env, &callback_class)?;
        let callback_class = env.new_global_ref(callback_class)?;
        let upload_provider_class = env.find_class(bindings.upload_provider_class)?;
        let upload_provider_class = env.new_global_ref(upload_provider_class)?;
        Ok(Self {
            core: Arc::new(BackendCore {
                vm: Arc::new(env.get_java_vm()?),
                engine: env.new_global_ref(engine)?,
                executor: env.new_global_ref(executor)?,
                callback_class,
                upload_provider_class,
                bindings,
            }),
        })
    }

    #[cfg(feature = "async")]
    pub fn create_async_client(&self, options: ClientOptions) -> Result<AndroidAsyncClient> {
        validate_options(&options)?;
        Ok(AndroidAsyncClient {
            core: Arc::clone(&self.core),
            options,
        })
    }

    #[cfg(feature = "blocking")]
    pub fn create_blocking_client(&self, options: ClientOptions) -> Result<AndroidBlockingClient> {
        validate_options(&options)?;
        Ok(AndroidBlockingClient {
            core: Arc::clone(&self.core),
            options,
        })
    }
}

fn validate_options(options: &ClientOptions) -> Result<()> {
    if options.ignore_certificate_errors {
        return Err(unsupported(
            "certificate policy must be configured when the Android engine is created",
        ));
    }
    Ok(())
}

#[derive(Clone)]
struct PreparedRequest {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    disable_cache: bool,
}

fn prepare_request_parts<S>(
    options: &ClientOptions,
    request: nyquest_interface::Request<S>,
) -> Result<(PreparedRequest, Option<nyquest_interface::Body<S>>)> {
    let url = resolve_url(options.base_url.as_deref(), &request.relative_uri)?;
    let method = method_name(request.method);
    let mut headers = options.default_headers.clone();
    if let Some(user_agent) = &options.user_agent {
        headers.push(("User-Agent".into(), user_agent.clone()));
    }
    headers.extend(
        request
            .additional_headers
            .into_iter()
            .map(|(name, value)| (name.into_owned(), value.into_owned())),
    );
    Ok((
        PreparedRequest {
            url,
            method,
            headers,
            body: None,
            disable_cache: options.caching_behavior == CachingBehavior::Disabled,
        },
        request.body,
    ))
}

fn resolve_url(base_url: Option<&str>, relative_uri: &str) -> Result<String> {
    let parsed = match base_url {
        Some(base) => url::Url::parse(base)
            .and_then(|base| base.join(relative_uri))
            .map_err(|_| Error::InvalidUrl)?,
        None => url::Url::parse(relative_uri).map_err(|_| Error::InvalidUrl)?,
    };
    Ok(parsed.into())
}

fn method_name(method: Method) -> String {
    match method {
        Method::Get => "GET".into(),
        Method::Post => "POST".into(),
        Method::Put => "PUT".into(),
        Method::Delete => "DELETE".into(),
        Method::Patch => "PATCH".into(),
        Method::Head => "HEAD".into(),
        Method::Other(method) => method.into_owned(),
    }
}

fn encode_form(
    fields: Vec<(
        std::borrow::Cow<'static, str>,
        std::borrow::Cow<'static, str>,
    )>,
) -> Vec<u8> {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.extend_pairs(fields.iter().map(|(name, value)| (&**name, &**value)));
    serializer.finish().into_bytes()
}

fn set_body(prepared: &mut PreparedRequest, content_type: &str, body: Vec<u8>) {
    prepared
        .headers
        .push(("Content-Type".into(), content_type.into()));
    prepared.body = Some(body);
}

#[cfg(feature = "async")]
#[derive(Clone)]
pub struct AndroidAsyncClient {
    core: Arc<BackendCore>,
    options: ClientOptions,
}

#[cfg(feature = "async")]
impl fmt::Debug for AndroidAsyncClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AndroidAsyncClient")
            .field(&self.core.bindings.name)
            .finish()
    }
}

#[cfg(feature = "async")]
impl nyquest_interface::r#async::AsyncClient for AndroidAsyncClient {
    type Response = AndroidAsyncResponse;

    fn describe(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }

    async fn request(
        &self,
        request: nyquest_interface::r#async::Request,
    ) -> Result<Self::Response> {
        let prepared = prepare_async_request(&self.options, request).await?;
        let guard = self.core.start_request(
            prepared,
            self.options.follow_redirects,
            self.options.request_timeout,
        )?;
        let metadata = wait_for_response_async(&guard.state).await?;
        Ok(AndroidAsyncResponse {
            inner: ResponseCore::new(guard, metadata, self.options.max_response_buffer_size),
        })
    }
}

#[cfg(feature = "async")]
async fn prepare_async_request(
    options: &ClientOptions,
    request: nyquest_interface::r#async::Request,
) -> Result<PreparedRequest> {
    let (mut prepared, body) = prepare_request_parts(options, request)?;
    match body {
        None => {}
        Some(nyquest_interface::Body::Bytes {
            content,
            content_type,
        }) => set_body(&mut prepared, &content_type, content.into_owned()),
        Some(nyquest_interface::Body::Form { fields }) => {
            set_body(
                &mut prepared,
                "application/x-www-form-urlencoded",
                encode_form(fields),
            );
        }
        Some(nyquest_interface::Body::Stream {
            stream,
            content_type,
        }) => {
            #[cfg(feature = "async-stream")]
            {
                use futures_util::AsyncReadExt;
                let mut stream = stream;
                let mut body = Vec::new();
                stream.read_to_end(&mut body).await?;
                set_body(&mut prepared, &content_type, body);
            }
            #[cfg(not(feature = "async-stream"))]
            match stream {}
        }
    }
    Ok(prepared)
}

#[cfg(feature = "async")]
pub struct AndroidAsyncResponse {
    inner: ResponseCore,
}

#[cfg(feature = "async")]
impl nyquest_interface::r#async::AsyncResponse for AndroidAsyncResponse {
    fn describe(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AndroidAsyncResponse")
            .field("status", &self.inner.metadata.status)
            .finish()
    }

    fn status(&self) -> u16 {
        self.inner.metadata.status
    }

    fn content_length(&self) -> Option<u64> {
        self.inner.metadata.content_length
    }

    fn get_header(&self, header: &str) -> Result<Vec<String>> {
        Ok(self.inner.metadata.header(header))
    }

    async fn text(mut self: Pin<&mut Self>) -> Result<String> {
        let bytes = self.as_mut().bytes().await?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    async fn bytes(mut self: Pin<&mut Self>) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        loop {
            let chunk = poll_fn(|cx| self.inner.poll_chunk(cx)).await?;
            let Some(chunk) = chunk else { break };
            if self
                .inner
                .max_response_buffer_size
                .is_some_and(|max| bytes.len() + chunk.len() > max as usize)
            {
                return Err(Error::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

#[cfg(feature = "async-stream")]
impl nyquest_interface::r#async::futures_io::AsyncRead for AndroidAsyncResponse {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.inner.poll_read(cx, buffer)
    }
}

#[cfg(feature = "blocking")]
#[derive(Clone)]
pub struct AndroidBlockingClient {
    core: Arc<BackendCore>,
    options: ClientOptions,
}

#[cfg(feature = "blocking")]
impl fmt::Debug for AndroidBlockingClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AndroidBlockingClient")
            .field(&self.core.bindings.name)
            .finish()
    }
}

#[cfg(feature = "blocking")]
impl nyquest_interface::blocking::BlockingClient for AndroidBlockingClient {
    type Response = AndroidBlockingResponse;

    fn describe(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }

    fn request(&self, request: nyquest_interface::blocking::Request) -> Result<Self::Response> {
        let prepared = prepare_blocking_request(&self.options, request)?;
        let guard = self.core.start_request(
            prepared,
            self.options.follow_redirects,
            self.options.request_timeout,
        )?;
        let metadata = wait_for_response_blocking(&guard.state)?;
        Ok(AndroidBlockingResponse {
            inner: ResponseCore::new(guard, metadata, self.options.max_response_buffer_size),
        })
    }
}

#[cfg(feature = "blocking")]
fn prepare_blocking_request(
    options: &ClientOptions,
    request: nyquest_interface::blocking::Request,
) -> Result<PreparedRequest> {
    let (mut prepared, body) = prepare_request_parts(options, request)?;
    match body {
        None => {}
        Some(nyquest_interface::Body::Bytes {
            content,
            content_type,
        }) => set_body(&mut prepared, &content_type, content.into_owned()),
        Some(nyquest_interface::Body::Form { fields }) => {
            set_body(
                &mut prepared,
                "application/x-www-form-urlencoded",
                encode_form(fields),
            );
        }
        Some(nyquest_interface::Body::Stream {
            mut stream,
            content_type,
        }) => {
            #[cfg(feature = "blocking-stream")]
            {
                use std::io::Read;
                let mut body = Vec::new();
                stream.read_to_end(&mut body)?;
                set_body(&mut prepared, &content_type, body);
            }
            #[cfg(not(feature = "blocking-stream"))]
            match stream {}
        }
    }
    Ok(prepared)
}

#[cfg(feature = "blocking")]
pub struct AndroidBlockingResponse {
    inner: ResponseCore,
}

#[cfg(feature = "blocking")]
impl nyquest_interface::blocking::BlockingResponse for AndroidBlockingResponse {
    fn describe(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AndroidBlockingResponse")
            .field("status", &self.inner.metadata.status)
            .finish()
    }

    fn status(&self) -> u16 {
        self.inner.metadata.status
    }

    fn content_length(&self) -> Option<u64> {
        self.inner.metadata.content_length
    }

    fn get_header(&self, header: &str) -> Result<Vec<String>> {
        Ok(self.inner.metadata.header(header))
    }

    fn text(&mut self) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.bytes()?).into_owned())
    }

    fn bytes(&mut self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        while let Some(chunk) = self.inner.next_chunk_blocking()? {
            if self
                .inner
                .max_response_buffer_size
                .is_some_and(|max| bytes.len() + chunk.len() > max as usize)
            {
                return Err(Error::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

#[cfg(feature = "blocking-stream")]
impl io::Read for AndroidBlockingResponse {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.inner.read_blocking(buffer)
    }
}

#[derive(Clone)]
struct ResponseMetadata {
    status: u16,
    content_length: Option<u64>,
    headers: Vec<(String, String)>,
}

impl ResponseMetadata {
    fn header(&self, name: &str) -> Vec<String> {
        self.headers
            .iter()
            .filter(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.clone())
            .collect()
    }
}

#[derive(Clone)]
enum Failure {
    Timeout,
    Io(String),
}

impl Failure {
    fn into_error(self) -> Error {
        match self {
            Self::Timeout => Error::RequestTimeout,
            Self::Io(message) => Error::Io(io::Error::other(message)),
        }
    }
}

struct RequestState {
    data: Mutex<RequestStateData>,
    changed: Condvar,
    vm: Arc<JavaVM>,
}

struct RequestStateData {
    metadata: Option<ResponseMetadata>,
    callback: Option<GlobalRef>,
    chunks: VecDeque<Vec<u8>>,
    terminal: Option<std::result::Result<(), Failure>>,
    response_waker: Option<Waker>,
    read_waker: Option<Waker>,
    read_in_flight: bool,
    follow_redirects: bool,
}

impl RequestState {
    fn new(vm: Arc<JavaVM>, follow_redirects: bool) -> Self {
        Self {
            data: Mutex::new(RequestStateData {
                metadata: None,
                callback: None,
                chunks: VecDeque::new(),
                terminal: None,
                response_waker: None,
                read_waker: None,
                read_in_flight: false,
                follow_redirects,
            }),
            changed: Condvar::new(),
            vm,
        }
    }

    fn lock(&self) -> MutexGuard<'_, RequestStateData> {
        self.data
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn wake(data: &mut RequestStateData) {
        if let Some(waker) = data.response_waker.take() {
            waker.wake();
        }
        if let Some(waker) = data.read_waker.take() {
            waker.wake();
        }
    }

    fn fail(&self, failure: Failure) {
        let mut data = self.lock();
        if data.terminal.is_none() {
            data.terminal = Some(Err(failure));
            data.read_in_flight = false;
            Self::wake(&mut data);
            self.changed.notify_all();
        }
    }

    fn cancel_java(&self) {
        let callback = self.lock().callback.clone();
        let Some(callback) = callback else { return };
        if let Ok(mut env) = self.vm.attach_current_thread() {
            let _ = env.call_method(callback.as_obj(), "cancel", "()V", &[]);
            clear_java_exception(&mut env);
        }
    }

    fn request_read(&self) -> std::result::Result<(), Failure> {
        let callback = self
            .lock()
            .callback
            .clone()
            .ok_or_else(|| Failure::Io("Android request callback is unavailable".into()))?;
        let mut env = self
            .vm
            .attach_current_thread()
            .map_err(|error| Failure::Io(format!("failed to attach to JVM: {error}")))?;
        env.call_method(callback.as_obj(), "read", "()V", &[])
            .map_err(|error| java_failure(&mut env, "failed to read Android response", error))?;
        Ok(())
    }
}

struct RequestGuard {
    _id: i64,
    state: Arc<RequestState>,
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        if self.state.lock().terminal.is_none() {
            self.state.cancel_java();
        }
    }
}

struct ResponseCore {
    _guard: RequestGuard,
    metadata: ResponseMetadata,
    current_chunk: Vec<u8>,
    current_offset: usize,
    max_response_buffer_size: Option<u64>,
}

impl ResponseCore {
    fn new(
        guard: RequestGuard,
        metadata: ResponseMetadata,
        max_response_buffer_size: Option<u64>,
    ) -> Self {
        Self {
            _guard: guard,
            metadata,
            current_chunk: Vec::new(),
            current_offset: 0,
            max_response_buffer_size,
        }
    }

    fn state(&self) -> &Arc<RequestState> {
        &self._guard.state
    }

    fn take_buffered(&mut self, output: &mut [u8]) -> usize {
        let remaining = &self.current_chunk[self.current_offset..];
        let count = output.len().min(remaining.len());
        output[..count].copy_from_slice(&remaining[..count]);
        self.current_offset += count;
        if self.current_offset == self.current_chunk.len() {
            self.current_chunk.clear();
            self.current_offset = 0;
        }
        count
    }

    fn begin_read_if_needed(&self, data: &mut RequestStateData) -> bool {
        if data.read_in_flight || data.terminal.is_some() {
            false
        } else {
            data.read_in_flight = true;
            true
        }
    }

    #[cfg(feature = "async")]
    fn poll_chunk(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<Option<Vec<u8>>>> {
        let should_read = {
            let mut data = self.state().lock();
            if let Some(chunk) = data.chunks.pop_front() {
                return Poll::Ready(Ok(Some(chunk)));
            }
            if let Some(terminal) = data.terminal.clone() {
                return Poll::Ready(terminal.map(|()| None).map_err(Failure::into_error));
            }
            data.read_waker = Some(cx.waker().clone());
            self.begin_read_if_needed(&mut data)
        };
        if should_read {
            if let Err(failure) = self.state().request_read() {
                self.state().fail(failure);
                return self.poll_chunk(cx);
            }
        }
        Poll::Pending
    }

    #[cfg(feature = "async-stream")]
    fn poll_read(
        &mut self,
        cx: &mut std::task::Context<'_>,
        output: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let count = self.take_buffered(output);
        if count > 0 || output.is_empty() {
            return Poll::Ready(Ok(count));
        }
        match self.poll_chunk(cx) {
            Poll::Ready(Ok(Some(chunk))) => {
                self.current_chunk = chunk;
                Poll::Ready(Ok(self.take_buffered(output)))
            }
            Poll::Ready(Ok(None)) => Poll::Ready(Ok(0)),
            Poll::Ready(Err(error)) => Poll::Ready(Err(error_to_io(error))),
            Poll::Pending => Poll::Pending,
        }
    }

    #[cfg(feature = "blocking")]
    fn next_chunk_blocking(&mut self) -> Result<Option<Vec<u8>>> {
        loop {
            let should_read = {
                let mut data = self.state().lock();
                if let Some(chunk) = data.chunks.pop_front() {
                    return Ok(Some(chunk));
                }
                if let Some(terminal) = data.terminal.clone() {
                    return terminal.map(|()| None).map_err(Failure::into_error);
                }
                self.begin_read_if_needed(&mut data)
            };
            if should_read {
                if let Err(failure) = self.state().request_read() {
                    self.state().fail(failure);
                    continue;
                }
            }
            let data = self.state().lock();
            drop(
                self.state()
                    .changed
                    .wait(data)
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
            );
        }
    }

    #[cfg(feature = "blocking-stream")]
    fn read_blocking(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = self.take_buffered(output);
        if count > 0 || output.is_empty() {
            return Ok(count);
        }
        match self.next_chunk_blocking().map_err(error_to_io)? {
            Some(chunk) => {
                self.current_chunk = chunk;
                Ok(self.take_buffered(output))
            }
            None => Ok(0),
        }
    }
}

impl BackendCore {
    fn start_request(
        self: &Arc<Self>,
        prepared: PreparedRequest,
        follow_redirects: bool,
        request_timeout: Option<Duration>,
    ) -> Result<RequestGuard> {
        let state = Arc::new(RequestState::new(Arc::clone(&self.vm), follow_redirects));
        let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        registry_lock().insert(id, Arc::clone(&state));
        let result = self.start_java_request(id, &state, prepared);
        if let Err(error) = result {
            registry_lock().remove(&id);
            return Err(error);
        }
        if let Some(timeout) = request_timeout {
            let state = Arc::downgrade(&state);
            thread::spawn(move || {
                thread::sleep(timeout);
                let Some(state) = state.upgrade() else { return };
                if state.lock().terminal.is_none() {
                    state.fail(Failure::Timeout);
                    state.cancel_java();
                    finish_request(id);
                }
            });
        }
        Ok(RequestGuard { _id: id, state })
    }

    fn start_java_request(
        &self,
        id: i64,
        state: &Arc<RequestState>,
        prepared: PreparedRequest,
    ) -> Result<()> {
        let mut env = self
            .vm
            .attach_current_thread()
            .map_err(|error| jni_io("failed to attach to JVM", error))?;
        let callback_class: &JClass<'_> = self.callback_class.as_obj().into();
        let callback = env
            .new_object(
                callback_class,
                self.bindings.callback_constructor,
                &[JValue::Long(id)],
            )
            .map_err(|error| java_error(&mut env, "failed to create Android callback", error))?;
        let url = env
            .new_string(&prepared.url)
            .map_err(|error| java_error(&mut env, "failed to allocate request URL", error))?;
        let callback_obj = callback;
        let builder = if self.bindings.callback_before_executor {
            env.call_method(
                self.engine.as_obj(),
                "newUrlRequestBuilder",
                self.bindings.engine_builder_signature,
                &[
                    JValue::Object(url.as_ref()),
                    JValue::Object(&callback_obj),
                    JValue::Object(self.executor.as_obj()),
                ],
            )
        } else {
            env.call_method(
                self.engine.as_obj(),
                "newUrlRequestBuilder",
                self.bindings.engine_builder_signature,
                &[
                    JValue::Object(url.as_ref()),
                    JValue::Object(self.executor.as_obj()),
                    JValue::Object(&callback_obj),
                ],
            )
        }
        .and_then(|value| value.l())
        .map_err(|error| java_error(&mut env, "failed to create Android request builder", error))?;

        let method = env
            .new_string(&prepared.method)
            .map_err(|error| java_error(&mut env, "failed to allocate HTTP method", error))?;
        let builder_return = format!("(Ljava/lang/String;)L{};", self.bindings.builder_class);
        env.call_method(
            &builder,
            "setHttpMethod",
            &builder_return,
            &[JValue::Object(method.as_ref())],
        )
        .map_err(|error| java_error(&mut env, "failed to set HTTP method", error))?;

        let header_return = format!(
            "(Ljava/lang/String;Ljava/lang/String;)L{};",
            self.bindings.builder_class
        );
        for (name, value) in prepared.headers {
            let name = env
                .new_string(name)
                .map_err(|error| java_error(&mut env, "failed to allocate header name", error))?;
            let value = env
                .new_string(value)
                .map_err(|error| java_error(&mut env, "failed to allocate header value", error))?;
            env.call_method(
                &builder,
                "addHeader",
                &header_return,
                &[
                    JValue::Object(name.as_ref()),
                    JValue::Object(value.as_ref()),
                ],
            )
            .map_err(|error| java_error(&mut env, "failed to add request header", error))?;
        }

        if prepared.disable_cache {
            let arguments = [JValue::Bool(JNI_TRUE)];
            let arguments = if self.bindings.disable_cache_takes_boolean {
                &arguments[..]
            } else {
                &[]
            };
            env.call_method(
                &builder,
                self.bindings.disable_cache_method,
                self.bindings.disable_cache_signature,
                arguments,
            )
            .map_err(|error| java_error(&mut env, "failed to disable request cache", error))?;
        }

        if let Some(body) = prepared.body {
            let bytes = env
                .byte_array_from_slice(&body)
                .map_err(|error| java_error(&mut env, "failed to allocate upload body", error))?;
            let upload_class: &JClass<'_> = self.upload_provider_class.as_obj().into();
            let upload = env
                .new_object(upload_class, "([B)V", &[JValue::Object(bytes.as_ref())])
                .map_err(|error| java_error(&mut env, "failed to create upload provider", error))?;
            let signature = format!(
                "({}Ljava/util/concurrent/Executor;)L{};",
                self.bindings.upload_provider_signature, self.bindings.builder_class
            );
            env.call_method(
                &builder,
                "setUploadDataProvider",
                &signature,
                &[
                    JValue::Object(&upload),
                    JValue::Object(self.executor.as_obj()),
                ],
            )
            .map_err(|error| java_error(&mut env, "failed to set upload provider", error))?;
        }

        let request_signature = format!("()L{};", self.bindings.request_class);
        let request = env
            .call_method(&builder, "build", &request_signature, &[])
            .and_then(|value| value.l())
            .map_err(|error| java_error(&mut env, "failed to build Android request", error))?;
        let set_request_signature = format!("(L{};)V", self.bindings.request_class);
        env.call_method(
            &callback_obj,
            "setRequest",
            &set_request_signature,
            &[JValue::Object(&request)],
        )
        .map_err(|error| java_error(&mut env, "failed to retain Android request", error))?;
        state.lock().callback =
            Some(env.new_global_ref(&callback_obj).map_err(|error| {
                java_error(&mut env, "failed to retain Android callback", error)
            })?);
        env.call_method(&request, "start", "()V", &[])
            .map_err(|error| java_error(&mut env, "failed to start Android request", error))?;
        Ok(())
    }
}

#[cfg(feature = "async")]
async fn wait_for_response_async(state: &Arc<RequestState>) -> Result<ResponseMetadata> {
    poll_fn(|cx| {
        let mut data = state.lock();
        if let Some(metadata) = data.metadata.clone() {
            return Poll::Ready(Ok(metadata));
        }
        if let Some(Err(failure)) = data.terminal.clone() {
            return Poll::Ready(Err(failure.into_error()));
        }
        data.response_waker = Some(cx.waker().clone());
        Poll::Pending
    })
    .await
}

#[cfg(feature = "blocking")]
fn wait_for_response_blocking(state: &Arc<RequestState>) -> Result<ResponseMetadata> {
    loop {
        let data = state.lock();
        if let Some(metadata) = data.metadata.clone() {
            return Ok(metadata);
        }
        if let Some(Err(failure)) = data.terminal.clone() {
            return Err(failure.into_error());
        }
        drop(
            state
                .changed
                .wait(data)
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
    }
}

static NEXT_REQUEST_ID: AtomicI64 = AtomicI64::new(1);
static REQUESTS: OnceLock<Mutex<HashMap<i64, Arc<RequestState>>>> = OnceLock::new();

fn registry_lock() -> MutexGuard<'static, HashMap<i64, Arc<RequestState>>> {
    REQUESTS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn request_state(handle: jlong) -> Option<Arc<RequestState>> {
    registry_lock().get(&handle).cloned()
}

fn finish_request(handle: jlong) {
    registry_lock().remove(&handle);
}

fn register_callback_natives(
    env: &mut JNIEnv<'_>,
    callback_class: &JClass<'_>,
) -> jni::errors::Result<()> {
    let methods = [
        NativeMethod {
            name: "nativeOnRedirect".into(),
            sig: "(JI[Ljava/lang/String;)Z".into(),
            fn_ptr: native_on_redirect as *mut c_void,
        },
        NativeMethod {
            name: "nativeOnResponseStarted".into(),
            sig: "(JIJ[Ljava/lang/String;)V".into(),
            fn_ptr: native_on_response_started as *mut c_void,
        },
        NativeMethod {
            name: "nativeOnReadCompleted".into(),
            sig: "(J[B)V".into(),
            fn_ptr: native_on_read_completed as *mut c_void,
        },
        NativeMethod {
            name: "nativeOnSucceeded".into(),
            sig: "(J)V".into(),
            fn_ptr: native_on_succeeded as *mut c_void,
        },
        NativeMethod {
            name: "nativeOnFailed".into(),
            sig: "(JLjava/lang/String;Z)V".into(),
            fn_ptr: native_on_failed as *mut c_void,
        },
        NativeMethod {
            name: "nativeOnCanceled".into(),
            sig: "(J)V".into(),
            fn_ptr: native_on_canceled as *mut c_void,
        },
    ];
    env.register_native_methods(callback_class, &methods)
}

unsafe extern "system" fn native_on_redirect(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    status: jint,
    headers: JObjectArray<'_>,
) -> jboolean {
    let Some(state) = request_state(handle) else {
        return JNI_FALSE;
    };
    let follow = state.lock().follow_redirects;
    if !follow {
        let metadata = response_metadata(&mut env, status, -1, &headers);
        let mut data = state.lock();
        data.metadata = Some(metadata);
        RequestState::wake(&mut data);
        state.changed.notify_all();
    }
    if follow {
        JNI_TRUE
    } else {
        JNI_FALSE
    }
}

unsafe extern "system" fn native_on_response_started(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    status: jint,
    content_length: jlong,
    headers: JObjectArray<'_>,
) {
    let Some(state) = request_state(handle) else {
        return;
    };
    let metadata = response_metadata(&mut env, status, content_length, &headers);
    let mut data = state.lock();
    data.metadata = Some(metadata);
    RequestState::wake(&mut data);
    state.changed.notify_all();
}

unsafe extern "system" fn native_on_read_completed(
    env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    bytes: JByteArray<'_>,
) {
    let Some(state) = request_state(handle) else {
        return;
    };
    match env.convert_byte_array(bytes) {
        Ok(bytes) => {
            let mut data = state.lock();
            data.read_in_flight = false;
            data.chunks.push_back(bytes);
            RequestState::wake(&mut data);
            state.changed.notify_all();
        }
        Err(error) => state.fail(Failure::Io(format!(
            "failed to copy Android response bytes: {error}"
        ))),
    }
}

unsafe extern "system" fn native_on_succeeded(_env: JNIEnv<'_>, _class: JClass<'_>, handle: jlong) {
    if let Some(state) = request_state(handle) {
        let mut data = state.lock();
        data.terminal = Some(Ok(()));
        data.read_in_flight = false;
        RequestState::wake(&mut data);
        state.changed.notify_all();
    }
    finish_request(handle);
}

unsafe extern "system" fn native_on_failed(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    message: JString<'_>,
    timed_out: jboolean,
) {
    if let Some(state) = request_state(handle) {
        let message = env
            .get_string(&message)
            .map(|value| value.into())
            .unwrap_or_else(|_| "Android request failed".into());
        state.fail(if timed_out == JNI_TRUE {
            Failure::Timeout
        } else {
            Failure::Io(message)
        });
    }
    finish_request(handle);
}

unsafe extern "system" fn native_on_canceled(_env: JNIEnv<'_>, _class: JClass<'_>, handle: jlong) {
    if let Some(state) = request_state(handle) {
        let mut data = state.lock();
        if data.terminal.is_none() {
            data.terminal = if data.metadata.is_some() && !data.follow_redirects {
                Some(Ok(()))
            } else {
                Some(Err(Failure::Io("Android request was canceled".into())))
            };
            data.read_in_flight = false;
            RequestState::wake(&mut data);
            state.changed.notify_all();
        }
    }
    finish_request(handle);
}

fn response_metadata(
    env: &mut JNIEnv<'_>,
    status: jint,
    content_length: jlong,
    headers: &JObjectArray<'_>,
) -> ResponseMetadata {
    ResponseMetadata {
        status: u16::try_from(status).unwrap_or_default(),
        content_length: u64::try_from(content_length).ok(),
        headers: string_pairs(env, headers),
    }
}

fn string_pairs(env: &mut JNIEnv<'_>, values: &JObjectArray<'_>) -> Vec<(String, String)> {
    let Ok(length) = env.get_array_length(values) else {
        return Vec::new();
    };
    let mut pairs = Vec::with_capacity(length as usize / 2);
    let mut index = 0;
    while index + 1 < length {
        let name = env
            .get_object_array_element(values, index)
            .ok()
            .and_then(|value| env.get_string(&JString::from(value)).ok().map(Into::into));
        let value = env
            .get_object_array_element(values, index + 1)
            .ok()
            .and_then(|value| env.get_string(&JString::from(value)).ok().map(Into::into));
        if let (Some(name), Some(value)) = (name, value) {
            pairs.push((name, value));
        }
        index += 2;
    }
    pairs
}

fn java_error(env: &mut JNIEnv<'_>, context: &str, error: jni::errors::Error) -> Error {
    Error::Io(io::Error::other(
        java_failure(env, context, error).message(),
    ))
}

fn java_failure(env: &mut JNIEnv<'_>, context: &str, error: jni::errors::Error) -> Failure {
    clear_java_exception(env);
    Failure::Io(format!("{context}: {error}"))
}

impl Failure {
    fn message(self) -> String {
        match self {
            Self::Timeout => "request timed out".into(),
            Self::Io(message) => message,
        }
    }
}

fn clear_java_exception(env: &mut JNIEnv<'_>) {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
}

fn jni_io(context: &str, error: jni::errors::Error) -> Error {
    Error::Io(io::Error::other(format!("{context}: {error}")))
}

fn unsupported(message: &str) -> Error {
    Error::Io(io::Error::new(io::ErrorKind::Unsupported, message))
}

fn error_to_io(error: Error) -> io::Error {
    match error {
        Error::Io(error) => error,
        Error::RequestTimeout => io::Error::new(io::ErrorKind::TimedOut, error),
        other => io::Error::other(other),
    }
}
