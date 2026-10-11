package io.openaide.android;

import java.net.URI;

/** The origin the workspace WebView loads: the app's own gateway, one port per workspace. */
final class ConnectionProfile {
    /** Fixed so the bundled Frontend keeps one origin per workspace, and with it its stored drafts and preferences. */
    static final int LOCAL_PORT = 5476;
    static final int PAIRED_PORT = 5475;
    final String endpoint;
    final boolean local;

    private ConnectionProfile(int port, boolean local) {
        this.endpoint = "http://127.0.0.1:" + port + "/";
        this.local = local;
    }

    /** The App Server in Termux. The gateway holds its token; the WebView holds no credential for it. */
    static ConnectionProfile local() { return new ConnectionProfile(LOCAL_PORT, true); }

    /** A paired computer is trusted by key; the WebView holds no credential for it. */
    static ConnectionProfile paired() { return new ConnectionProfile(PAIRED_PORT, false); }

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
