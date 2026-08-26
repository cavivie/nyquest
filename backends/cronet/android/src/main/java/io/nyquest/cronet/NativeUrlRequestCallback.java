package io.nyquest.cronet;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.util.List;
import java.util.Map;
import org.chromium.net.CronetException;
import org.chromium.net.NetworkException;
import org.chromium.net.UploadDataProvider;
import org.chromium.net.UploadDataSink;
import org.chromium.net.UrlRequest;
import org.chromium.net.UrlResponseInfo;

/** JNI adapter between Cronet callbacks and the Nyquest Rust backend. */
public final class NativeUrlRequestCallback extends UrlRequest.Callback {
    private static final int READ_BUFFER_SIZE = 64 * 1024;

    private long handle;
    private UrlRequest request;
    private final ByteBuffer readBuffer = ByteBuffer.allocateDirect(READ_BUFFER_SIZE);

    public NativeUrlRequestCallback(long handle) {
        this.handle = handle;
    }

    public void setRequest(UrlRequest request) {
        this.request = request;
    }

    @Override
    public void onRedirectReceived(UrlRequest request, UrlResponseInfo info, String newLocationUrl) {
        this.request = request;
        if (nativeOnRedirect(handle, info.getHttpStatusCode(), flattenHeaders(info))) {
            request.followRedirect();
        } else {
            request.cancel();
        }
    }

    @Override
    public void onResponseStarted(UrlRequest request, UrlResponseInfo info) {
        this.request = request;
        nativeOnResponseStarted(handle, info.getHttpStatusCode(), contentLength(info), flattenHeaders(info));
    }

    @Override
    public void onReadCompleted(UrlRequest request, UrlResponseInfo info, ByteBuffer buffer) {
        buffer.flip();
        byte[] bytes = new byte[buffer.remaining()];
        buffer.get(bytes);
        nativeOnReadCompleted(handle, bytes);
    }

    @Override
    public void onSucceeded(UrlRequest request, UrlResponseInfo info) {
        nativeOnSucceeded(handle);
        handle = 0;
    }

    @Override
    public void onFailed(UrlRequest request, UrlResponseInfo info, CronetException error) {
        boolean timedOut = error instanceof NetworkException
                && ((NetworkException) error).getErrorCode() == NetworkException.ERROR_TIMED_OUT;
        nativeOnFailed(handle, error.toString(), timedOut);
        handle = 0;
    }

    @Override
    public void onCanceled(UrlRequest request, UrlResponseInfo info) {
        nativeOnCanceled(handle);
        handle = 0;
    }

    public void read() {
        readBuffer.clear();
        request.read(readBuffer);
    }

    public void cancel() {
        if (request != null && !request.isDone()) {
            request.cancel();
        }
    }

    private static String[] flattenHeaders(UrlResponseInfo info) {
        List<Map.Entry<String, String>> headers = info.getAllHeadersAsList();
        String[] flattened = new String[headers.size() * 2];
        int index = 0;
        for (Map.Entry<String, String> header : headers) {
            flattened[index++] = header.getKey();
            flattened[index++] = header.getValue();
        }
        return flattened;
    }

    private static long contentLength(UrlResponseInfo info) {
        List<String> values = info.getAllHeaders().get("content-length");
        if (values == null) {
            values = info.getAllHeaders().get("Content-Length");
        }
        if (values == null || values.isEmpty()) {
            return -1;
        }
        try {
            return Long.parseLong(values.get(0));
        } catch (NumberFormatException ignored) {
            return -1;
        }
    }

    private static native boolean nativeOnRedirect(long handle, int status, String[] headers);
    private static native void nativeOnResponseStarted(long handle, int status, long contentLength, String[] headers);
    private static native void nativeOnReadCompleted(long handle, byte[] bytes);
    private static native void nativeOnSucceeded(long handle);
    private static native void nativeOnFailed(long handle, String message, boolean timedOut);
    private static native void nativeOnCanceled(long handle);

    /** Upload provider for request bodies already materialized by Rust. */
    public static final class ByteArrayUploadProvider extends UploadDataProvider {
        private final byte[] bytes;
        private int position;

        public ByteArrayUploadProvider(byte[] bytes) {
            this.bytes = bytes;
        }

        @Override
        public long getLength() {
            return bytes.length;
        }

        @Override
        public void read(UploadDataSink sink, ByteBuffer buffer) {
            int count = Math.min(buffer.remaining(), bytes.length - position);
            buffer.put(bytes, position, count);
            position += count;
            sink.onReadSucceeded(false);
        }

        @Override
        public void rewind(UploadDataSink sink) {
            position = 0;
            sink.onRewindSucceeded();
        }

        @Override
        public void close() throws IOException {}
    }
}
