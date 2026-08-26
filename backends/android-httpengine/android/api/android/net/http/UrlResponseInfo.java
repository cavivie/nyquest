package android.net.http;

import java.util.List;
import java.util.Map;

public abstract class UrlResponseInfo {
    public abstract int getHttpStatusCode();
    public abstract HeaderBlock getHeaders();

    public abstract static class HeaderBlock {
        public abstract List<Map.Entry<String, String>> getAsList();
        public abstract Map<String, List<String>> getAsMap();
    }
}
