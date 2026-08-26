package io.nyquest.runtime;

import android.app.Activity;
import android.os.Bundle;
import android.util.Log;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import org.chromium.net.CronetEngine;

public final class RuntimeTestActivity extends Activity {
    private static final String TAG = "NyquestRuntime";

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        CronetEngine engine = new CronetEngine.Builder(this).build();
        run(engine);
    }

    private void run(CronetEngine engine) {
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
