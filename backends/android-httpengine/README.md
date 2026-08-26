<div class="rustdoc-hidden">

# nyquest-backend-android-httpengine

</div>

Android [`android.net.http.HttpEngine`] backend for [`nyquest`]. The application creates and
configures the engine, then passes it and a callback `Executor` to `HttpEngineBackend::new` through
JNI.

`HttpEngine` is available on Android API 34 or Android S extension 7. Applications supporting
older Android versions must check availability before constructing this backend and may choose a
different backend, such as `nyquest-backend-cronet`, for those devices.

Add the Java adapter under `android/src/main/java` to the Android application source set. The
Rust JNI bindings are generated automatically by `build.rs` from the adapter and the minimal Java
API declarations under `android/api`; generated Rust source is not checked in. The
backend supports async and blocking clients, buffered uploads, streaming downloads, cancellation,
redirects, and request timeouts. Rust streaming upload sources are collected before the Java
request starts in this initial implementation.

## Features

- `async`
- `async-stream`
- `blocking`
- `blocking-stream`

[`android.net.http.HttpEngine`]: https://developer.android.com/reference/android/net/http/HttpEngine
[`nyquest`]: https://docs.rs/nyquest
