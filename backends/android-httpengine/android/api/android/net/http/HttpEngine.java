package android.net.http;

import java.util.concurrent.Executor;

public abstract class HttpEngine {
    public abstract UrlRequest.Builder newUrlRequestBuilder(
            String url, Executor executor, UrlRequest.Callback callback);
}
