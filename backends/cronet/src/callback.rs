use jni::objects::{JByteArray, JClass, JObjectArray, JString};
use jni::sys::{jboolean, jint, jlong, JNI_FALSE, JNI_TRUE};
use jni::Env;

use crate::bindings::io::nyquest::cronet::{
    NativeUrlRequestCallbackAPI, NativeUrlRequestCallbackNativeInterface,
};
use crate::error::Failure;
use crate::state::{finish_request, request_state, RequestState, ResponseMetadata};

impl NativeUrlRequestCallbackNativeInterface for NativeUrlRequestCallbackAPI {
    type Error = jni::errors::Error;

    fn native_on_redirect<'local>(
        env: &mut Env<'local>,
        _class: JClass<'local>,
        handle: jlong,
        status: jint,
        headers: JObjectArray<'local, JString<'local>>,
    ) -> Result<jboolean, Self::Error> {
        let Some(state) = request_state(handle) else {
            return Ok(JNI_FALSE);
        };
        let follow = state.lock().follow_redirects;
        if !follow {
            let metadata = response_metadata(env, status, -1, &headers);
            let mut data = state.lock();
            data.metadata = Some(metadata);
            RequestState::wake(&mut data);
            state.changed.notify_all();
        }
        Ok(if follow { JNI_TRUE } else { JNI_FALSE })
    }

    fn native_on_response_started<'local>(
        env: &mut Env<'local>,
        _class: JClass<'local>,
        handle: jlong,
        status: jint,
        content_length: jlong,
        headers: JObjectArray<'local, JString<'local>>,
    ) -> Result<(), Self::Error> {
        if let Some(state) = request_state(handle) {
            let metadata = response_metadata(env, status, content_length, &headers);
            let mut data = state.lock();
            data.metadata = Some(metadata);
            RequestState::wake(&mut data);
            state.changed.notify_all();
        }
        Ok(())
    }

    fn native_on_read_completed<'local>(
        env: &mut Env<'local>,
        _class: JClass<'local>,
        handle: jlong,
        bytes: JByteArray<'local>,
    ) -> Result<(), Self::Error> {
        if let Some(state) = request_state(handle) {
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
        Ok(())
    }

    fn native_on_succeeded<'local>(
        _env: &mut Env<'local>,
        _class: JClass<'local>,
        handle: jlong,
    ) -> Result<(), Self::Error> {
        if let Some(state) = request_state(handle) {
            let mut data = state.lock();
            data.terminal = Some(Ok(()));
            data.read_in_flight = false;
            RequestState::wake(&mut data);
            state.changed.notify_all();
        }
        finish_request(handle);
        Ok(())
    }

    fn native_on_failed<'local>(
        env: &mut Env<'local>,
        _class: JClass<'local>,
        handle: jlong,
        message: JString<'local>,
        timed_out: jboolean,
    ) -> Result<(), Self::Error> {
        if let Some(state) = request_state(handle) {
            let message = message
                .try_to_string(env)
                .unwrap_or_else(|_| "Android request failed".into());
            state.fail(if timed_out == JNI_TRUE {
                Failure::Timeout
            } else {
                Failure::Io(message)
            });
        }
        finish_request(handle);
        Ok(())
    }

    fn native_on_canceled<'local>(
        _env: &mut Env<'local>,
        _class: JClass<'local>,
        handle: jlong,
    ) -> Result<(), Self::Error> {
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
        Ok(())
    }
}

fn response_metadata(
    env: &mut Env<'_>,
    status: jint,
    content_length: jlong,
    headers: &JObjectArray<'_, JString<'_>>,
) -> ResponseMetadata {
    ResponseMetadata {
        status: u16::try_from(status).unwrap_or_default(),
        content_length: u64::try_from(content_length).ok(),
        headers: string_pairs(env, headers),
    }
}

fn string_pairs(
    env: &mut Env<'_>,
    values: &JObjectArray<'_, JString<'_>>,
) -> Vec<(String, String)> {
    let Ok(length) = values.len(env) else {
        return Vec::new();
    };
    let mut pairs = Vec::with_capacity(length / 2);
    let mut index = 0;
    while index + 1 < length {
        let name = values
            .get_element(env, index)
            .ok()
            .and_then(|value| value.try_to_string(env).ok());
        let value = values
            .get_element(env, index + 1)
            .ok()
            .and_then(|value| value.try_to_string(env).ok());
        if let (Some(name), Some(value)) = (name, value) {
            pairs.push((name, value));
        }
        index += 2;
    }
    pairs
}
