use std::ffi::c_void;

use jni::objects::{JByteArray, JClass, JObjectArray, JString};
use jni::sys::{jboolean, jint, jlong, JNI_FALSE, JNI_TRUE};
use jni::{JNIEnv, NativeMethod};

use crate::error::Failure;
use crate::state::{finish_request, request_state, RequestState, ResponseMetadata};

pub(crate) fn register_callback_natives(
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
