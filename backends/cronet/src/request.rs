use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use jni::objects::{JClass, JValue};
use jni::sys::JNI_TRUE;
use nyquest_interface::client::{CachingBehavior, ClientOptions};
use nyquest_interface::{Error, Method, Result};

use crate::backend::BackendCore;
use crate::error::{java_error, jni_io, Failure};
use crate::state::{finish_request, registry_lock, RequestGuard, RequestState, NEXT_REQUEST_ID};

#[derive(Clone)]
pub(crate) struct PreparedRequest {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    disable_cache: bool,
}

pub(crate) fn prepare_request_parts<S>(
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

pub(crate) fn encode_form(
    fields: Vec<(
        std::borrow::Cow<'static, str>,
        std::borrow::Cow<'static, str>,
    )>,
) -> Vec<u8> {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.extend_pairs(fields.iter().map(|(name, value)| (&**name, &**value)));
    serializer.finish().into_bytes()
}

pub(crate) fn set_body(prepared: &mut PreparedRequest, content_type: &str, body: Vec<u8>) {
    prepared
        .headers
        .push(("Content-Type".into(), content_type.into()));
    prepared.body = Some(body);
}

impl BackendCore {
    pub(crate) fn start_request(
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
