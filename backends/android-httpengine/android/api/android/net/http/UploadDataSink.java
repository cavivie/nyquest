package android.net.http;

public abstract class UploadDataSink {
    public abstract void onReadSucceeded(boolean finalChunk);
    public abstract void onRewindSucceeded();
}
