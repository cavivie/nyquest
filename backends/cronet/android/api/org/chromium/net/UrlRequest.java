package org.chromium.net;

import java.nio.ByteBuffer;
import java.util.concurrent.Executor;

public abstract class UrlRequest {
    protected UrlRequest() {}

    public abstract void start();
    public abstract void followRedirect();
    public abstract void read(ByteBuffer buffer);
    public abstract void cancel();
    public abstract boolean isDone();

    public abstract static class Callback {
        public abstract void onRedirectReceived(
                UrlRequest request, UrlResponseInfo info, String newLocationUrl);
        public abstract void onResponseStarted(UrlRequest request, UrlResponseInfo info);
        public abstract void onReadCompleted(
                UrlRequest request, UrlResponseInfo info, ByteBuffer buffer);
        public abstract void onSucceeded(UrlRequest request, UrlResponseInfo info);
        public abstract void onFailed(
                UrlRequest request, UrlResponseInfo info, CronetException error);
        public abstract void onCanceled(UrlRequest request, UrlResponseInfo info);
    }

    public abstract static class Builder {
        protected Builder() {}

        public abstract Builder setHttpMethod(String method);
        public abstract Builder addHeader(String header, String value);
        public abstract Builder disableCache();
        public abstract Builder setUploadDataProvider(
                UploadDataProvider uploadDataProvider, Executor executor);
        public abstract UrlRequest build();
    }
}
