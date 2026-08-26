package org.chromium.net;

public abstract class CronetException extends Exception {
    protected CronetException(String message, Throwable cause) { super(message, cause); }
}
