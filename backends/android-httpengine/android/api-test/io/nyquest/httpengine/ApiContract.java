package io.nyquest.httpengine;

import android.net.http.HttpEngine;
import android.net.http.UploadDataProvider;
import android.net.http.UrlRequest;
import java.nio.ByteBuffer;
import java.util.concurrent.Executor;

/** Compile-only coverage for the HttpEngine API surface used by the Rust bindings. */
final class ApiContract {
    private ApiContract() {}

    static void compile(
            HttpEngine engine,
            Executor executor,
            UrlRequest.Callback callback,
            UploadDataProvider uploadDataProvider)
            throws Exception {
        UrlRequest request = engine.newUrlRequestBuilder("https://example.com", executor, callback)
                .setHttpMethod("POST")
                .addHeader("Content-Type", "application/octet-stream")
                .setCacheDisabled(true)
                .setUploadDataProvider(uploadDataProvider, executor)
                .build();
        request.start();
        request.followRedirect();
        request.read(ByteBuffer.allocateDirect(1));
        request.cancel();
        request.isDone();
        uploadDataProvider.getLength();
    }
}
