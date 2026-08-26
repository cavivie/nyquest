package io.nyquest.cronet;

import java.nio.ByteBuffer;
import java.util.concurrent.Executor;
import org.chromium.net.CronetEngine;
import org.chromium.net.UploadDataProvider;
import org.chromium.net.UrlRequest;

/** Compile-only coverage for the Cronet API surface used by the Rust bindings. */
final class ApiContract {
    private ApiContract() {}

    static void compile(
            CronetEngine engine,
            Executor executor,
            UrlRequest.Callback callback,
            UploadDataProvider uploadDataProvider)
            throws Exception {
        UrlRequest request = engine.newUrlRequestBuilder("https://example.com", callback, executor)
                .setHttpMethod("POST")
                .addHeader("Content-Type", "application/octet-stream")
                .disableCache()
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
