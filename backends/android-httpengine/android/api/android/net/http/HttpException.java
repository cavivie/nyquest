package android.net.http;

public abstract class HttpException extends Exception {
    protected HttpException(String message, Throwable cause) { super(message, cause); }
}
