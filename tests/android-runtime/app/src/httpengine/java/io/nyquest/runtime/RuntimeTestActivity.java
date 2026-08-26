package io.nyquest.runtime;

import android.app.Activity;
import android.net.http.HttpEngine;
import android.os.Bundle;
import android.util.Log;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public final class RuntimeTestActivity extends Activity {
    private static final String TAG = "NyquestRuntime";

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        HttpEngine engine = new HttpEngine.Builder(this).build();
        run(engine);
    }

    private void run(HttpEngine engine) {
        ExecutorService executor = Executors.newSingleThreadExecutor();
        new Thread(() -> {
            String result = NativeRuntime.run(
                    engine, executor, "http://10.0.2.2:8765/runtime");
            Log.i(TAG, result);
            executor.shutdownNow();
            engine.shutdown();
            finish();
        }, "nyquest-runtime").start();
    }
}
