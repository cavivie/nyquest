#![cfg(target_os = "android")]

use std::time::Duration;

use jni::errors::ThrowRuntimeExAndDefault;
use jni::objects::{JClass, JObject, JString};
use jni::{Env, EnvUnowned};

#[cfg(all(feature = "cronet", feature = "httpengine"))]
compile_error!("enable exactly one Android backend feature");
#[cfg(not(any(feature = "cronet", feature = "httpengine")))]
compile_error!("enable exactly one Android backend feature");

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_nyquest_runtime_NativeRuntime_run<'local>(
    mut unowned_env: EnvUnowned<'local>,
    _class: JClass<'local>,
    engine: JObject<'local>,
    executor: JObject<'local>,
    url: JString<'local>,
) -> JString<'local> {
    unowned_env
        .with_env(|env| -> jni::errors::Result<JString<'local>> {
            let result = run_test(env, &engine, &executor, &url)
                .map(|summary| format!("NYQUEST_RUNTIME_PASS {summary}"))
                .unwrap_or_else(|error| format!("NYQUEST_RUNTIME_FAIL {error}"));
            JString::from_str(env, result)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

fn run_test(
    env: &mut Env<'_>,
    engine: &JObject<'_>,
    executor: &JObject<'_>,
    url: &JString<'_>,
) -> Result<String, String> {
    let url = url
        .try_to_string(env)
        .map_err(|error| format!("failed to read URL: {error}"))?;
    let backend = create_backend(env, engine, executor)
        .map_err(|error| format!("failed to initialize backend: {error}"))?;
    let client = nyquest::ClientBuilder::default()
        .blocking_backend(backend)
        .request_timeout(Duration::from_secs(15))
        .build_blocking()
        .map_err(|error| format!("failed to build client: {error}"))?;
    let response = client
        .request(nyquest::blocking::Request::get(url))
        .map_err(|error| format!("request failed: {error:?}"))?;
    let status = response.status();
    let marker = response
        .get_header("x-nyquest-runtime")
        .map_err(|error| format!("failed to read response header: {error}"))?;
    let body = response
        .text()
        .map_err(|error| format!("failed to read response body: {error}"))?;

    if status != 200 {
        return Err(format!("unexpected status: {status}"));
    }
    if marker != ["ok"] {
        return Err(format!("unexpected marker header: {marker:?}"));
    }
    if body != "nyquest-runtime-ok" {
        return Err(format!("unexpected body: {body:?}"));
    }

    Ok(format!("backend={} status={status}", backend_name()))
}

#[cfg(feature = "cronet")]
fn create_backend(
    env: &mut Env<'_>,
    engine: &JObject<'_>,
    executor: &JObject<'_>,
) -> jni::errors::Result<nyquest_backend_cronet::CronetBackend> {
    nyquest_backend_cronet::CronetBackend::new(env, engine, executor)
}

#[cfg(feature = "httpengine")]
fn create_backend(
    env: &mut Env<'_>,
    engine: &JObject<'_>,
    executor: &JObject<'_>,
) -> jni::errors::Result<nyquest_backend_android_httpengine::HttpEngineBackend> {
    nyquest_backend_android_httpengine::HttpEngineBackend::new(env, engine, executor)
}

#[cfg(feature = "cronet")]
fn backend_name() -> &'static str {
    "cronet"
}

#[cfg(feature = "httpengine")]
fn backend_name() -> &'static str {
    "httpengine"
}
