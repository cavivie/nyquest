package android.net.http;

import java.nio.ByteBuffer;
import java.util.concurrent.Executor;

public abstract class UrlRequest {
    protected UrlRequest() {}

    public abstract void start();
    public abstract void followRedirect();
    public abstract void read(ByteBuffer buffer);
    public abstract void cancel();
    public abstract boolean isDone();

    public interface Callback {
        void onRedirectReceived(UrlRequest request, UrlResponseInfo info, String newLocationUrl);
        void onResponseStarted(UrlRequest request, UrlResponseInfo info);
        void onReadCompleted(UrlRequest request, UrlResponseInfo info, ByteBuffer buffer);
        void onSucceeded(UrlRequest request, UrlResponseInfo info);
        void onFailed(UrlRequest request, UrlResponseInfo info, HttpException error);
        void onCanceled(UrlRequest request, UrlResponseInfo info);
    }

    public abstract static class Builder {
        protected Builder() {}

        public abstract Builder setHttpMethod(String method);
        public abstract Builder addHeader(String header, String value);
        public abstract Builder setCacheDisabled(boolean disabled);
        public abstract Builder setUploadDataProvider(
                UploadDataProvider uploadDataProvider, Executor executor);
        public abstract UrlRequest build();
    }
}
