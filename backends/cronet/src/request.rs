use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::bindings::io::nyquest::cronet::{
    NativeUrlRequestCallback, NativeUrlRequestCallbackByteArrayUploadProvider,
};
use crate::bindings::org::chromium::net::{UploadDataProvider, UrlRequestCallback};
use nyquest_interface::client::{CachingBehavior, ClientOptions};
use nyquest_interface::{Error, Method, Result};

use crate::backend::BackendCore;
use crate::error::{jni_io, Failure};
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
        self.vm
            .attach_current_thread(|env| -> jni::errors::Result<()> {
                let callback = NativeUrlRequestCallback::new(env, id)?;
                let url = env.new_string(&prepared.url)?;
                let builder = self.engine.new_url_request_builder(
                    env,
                    &url,
                    <NativeUrlRequestCallback<'_> as AsRef<UrlRequestCallback<'_>>>::as_ref(
                        &callback,
                    ),
                    self.executor.as_obj(),
                )?;

                let method = env.new_string(&prepared.method)?;
                builder.set_http_method(env, &method)?;

                for (name, value) in prepared.headers {
                    let name = env.new_string(name)?;
                    let value = env.new_string(value)?;
                    builder.add_header(env, &name, &value)?;
                }

                if prepared.disable_cache {
                    builder.disable_cache(env)?;
                }

                if let Some(body) = prepared.body {
                    let bytes = env.byte_array_from_slice(&body)?;
                    let upload = NativeUrlRequestCallbackByteArrayUploadProvider::new(env, &bytes)?;
                    let upload: UploadDataProvider<'_> = upload.into();
                    builder.set_upload_data_provider(env, &upload, self.executor.as_obj())?;
                }

                let request = builder.build(env)?;
                callback.set_request(env, &request)?;
                state.lock().callback = Some(env.new_global_ref(&callback)?);
                request.start(env)?;
                Ok(())
            })
            .map_err(|error| jni_io("failed to start Cronet request", error))
    }
}
