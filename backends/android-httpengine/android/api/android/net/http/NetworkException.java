package android.net.http;

public abstract class NetworkException extends HttpException {
    public static final int ERROR_TIMED_OUT = 4;
    protected NetworkException(String message, Throwable cause) { super(message, cause); }
    public abstract int getErrorCode();
}
