package org.chromium.net;

public abstract class NetworkException extends CronetException {
    public static final int ERROR_TIMED_OUT = 4;
    protected NetworkException(String message, Throwable cause) { super(message, cause); }
    public abstract int getErrorCode();
}
