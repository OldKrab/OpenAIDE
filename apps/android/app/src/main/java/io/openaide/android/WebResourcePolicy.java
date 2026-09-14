package io.openaide.android;

import java.net.URI;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;

final class WebResourcePolicy {
    private final Set<String> documents = ConcurrentHashMap.newKeySet();

    void allowDocument(String address) {
        URI uri = URI.create(address);
        if (!"content".equals(uri.getScheme()) || uri.getAuthority() == null) {
            throw new IllegalArgumentException("Only selected content documents are supported");
        }
        documents.add(address);
    }

    boolean allows(String address) {
        try {
            URI uri = URI.create(address);
            return "data".equals(uri.getScheme()) || "blob".equals(uri.getScheme())
                || documents.contains(address)
                || ("http".equals(uri.getScheme()) && "127.0.0.1".equals(uri.getHost())
                    && uri.getPort() == 5474 && uri.getUserInfo() == null);
        } catch (IllegalArgumentException error) {
            return false;
        }
    }

    void clear() { documents.clear(); }
}
