package org.chromium.net;

public abstract class UploadDataSink {
    public abstract void onReadSucceeded(boolean finalChunk);
    public abstract void onRewindSucceeded();
}
