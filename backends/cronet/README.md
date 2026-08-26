<div class="rustdoc-hidden">

# nyquest-backend-cronet

</div>

Android [Cronet] backend for [`nyquest`]. The application chooses and configures the Cronet
provider, then passes its `org.chromium.net.CronetEngine` and callback `Executor` to
`CronetBackend::new` through JNI. This crate does not add a transitive Cronet provider.

Add the Java adapter under `android/src/main/java` to the Android application source set and add
the desired Cronet implementation, for example `org.chromium.net:cronet-embedded`.

The backend supports async and blocking clients, buffered uploads, streaming downloads,
cancellation, redirects, and request timeouts. Rust streaming upload sources are collected before
the Java request starts in this initial implementation.

## Features

- `async`
- `async-stream`
- `blocking`
- `blocking-stream`

[Cronet]: https://developer.android.com/develop/connectivity/cronet
[`nyquest`]: https://docs.rs/nyquest
