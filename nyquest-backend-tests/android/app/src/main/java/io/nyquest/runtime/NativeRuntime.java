package io.nyquest.runtime;

import java.util.concurrent.Executor;

final class NativeRuntime {
    static {
        System.loadLibrary("nyquest_backend_tests_android");
    }

    private NativeRuntime() {}

    static native String run(Object engine, Executor executor, String url);
}
