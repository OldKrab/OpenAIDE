package io.openaide.android;

import java.net.URI;

/** The origin the workspace WebView loads: the Termux Web Shell, or the paired computer's gateway. */
final class ConnectionProfile {
    static final String LOCAL_ENDPOINT = "http://127.0.0.1:5474/";
    /** Fixed so the bundled Frontend keeps one origin, and with it its stored drafts and preferences. */
    static final int PAIRED_PORT = 5475;
    static final String PAIRED_ENDPOINT = "http://127.0.0.1:" + PAIRED_PORT + "/";
    final String endpoint;
    final String username;
    final String password;
    final boolean local;

    ConnectionProfile(String address, String username, String password, boolean local) {
        if (!(local ? LOCAL_ENDPOINT : PAIRED_ENDPOINT).equals(address)
                || (local && (username.isEmpty() || username.contains(":") || password.isEmpty()))) {
            throw new IllegalArgumentException("Unsupported workspace address or credentials.");
        }
        this.endpoint = address;
        this.username = username;
        this.password = password;
        this.local = local;
    }

    /** A paired computer is trusted by key; the WebView holds no credential for it. */
    static ConnectionProfile paired() { return new ConnectionProfile(PAIRED_ENDPOINT, "", "", false); }

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
