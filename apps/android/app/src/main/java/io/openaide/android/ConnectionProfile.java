package io.openaide.android;

import java.net.URI;

final class ConnectionProfile {
    final String endpoint;
    final String username;
    final String password;
    final boolean local;

    ConnectionProfile(String address, String username, String password, boolean local) {
        URI uri = URI.create(address.trim());
        if (uri.getHost() == null || uri.getUserInfo() != null || uri.getQuery() != null
                || uri.getFragment() != null || !(uri.getPath().isEmpty() || "/".equals(uri.getPath()))
                || (local ? !"http://127.0.0.1:5474/".equals(address) : !"https".equals(uri.getScheme()))
                || username.isEmpty() || username.contains(":") || password.isEmpty()) {
            throw new IllegalArgumentException("Use an HTTPS server address, username and password; no path or embedded credentials.");
        }
        this.endpoint = uri.resolve("/").toString();
        this.username = username;
        this.password = password;
        this.local = local;
    }

    boolean owns(String address) {
        try {
            URI candidate = URI.create(address);
            URI origin = URI.create(endpoint);
            return candidate.getUserInfo() == null && origin.getScheme().equals(candidate.getScheme())
                && origin.getHost().equalsIgnoreCase(candidate.getHost()) && port(origin) == port(candidate);
        } catch (RuntimeException error) { return false; }
    }

    private static int port(URI uri) {
        return uri.getPort() == -1 ? ("https".equals(uri.getScheme()) ? 443 : 80) : uri.getPort();
    }
}
