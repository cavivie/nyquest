package org.chromium.net;

import java.util.List;
import java.util.Map;

public abstract class UrlResponseInfo {
    public abstract int getHttpStatusCode();
    public abstract List<Map.Entry<String, String>> getAllHeadersAsList();
    public abstract Map<String, List<String>> getAllHeaders();
}
