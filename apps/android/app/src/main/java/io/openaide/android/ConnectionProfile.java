package io.openaide.android;

import java.net.URI;

/** The origin the workspace WebView loads: the app's own gateway, one port per kind of workspace. */
final class ConnectionProfile {
    /** Fixed so the bundled Frontend keeps one origin per workspace, and with it its stored drafts and preferences. */
    static final int LOCAL_PORT = 5476;
    static final int PAIRED_PORT = 5475;
    static final int SSH_PORT = 5477;

    /** Where the workspace's App Server runs, and so how the gateway reaches it. */
    enum Kind {
        /** In Termux on this phone. */
        LOCAL(LOCAL_PORT),
        /** On a paired computer, over iroh. */
        PAIRED(PAIRED_PORT),
        /** On an SSH Computer this phone started it on (ADR-0069). */
        SSH(SSH_PORT);

        final int port;
        Kind(int port) { this.port = port; }
    }

    final String endpoint;
    final Kind kind;
    /** The App Server in Termux, whose local tools this phone manages. */
    final boolean local;

    private ConnectionProfile(Kind kind) {
        this.endpoint = "http://127.0.0.1:" + kind.port + "/";
        this.kind = kind;
        this.local = kind == Kind.LOCAL;
    }

    /** The App Server in Termux. The gateway holds its token; the WebView holds no credential for it. */
    static ConnectionProfile local() { return new ConnectionProfile(Kind.LOCAL); }

    /** A paired computer is trusted by key; the WebView holds no credential for it. */
    static ConnectionProfile paired() { return new ConnectionProfile(Kind.PAIRED); }

    /** An SSH Computer. The gateway holds the token of its App Server; the WebView holds none. */
    static ConnectionProfile ssh() { return new ConnectionProfile(Kind.SSH); }

    /** This shell holds the App Server as a client, so the background service watches its Tasks. */
    boolean held() { return kind != Kind.PAIRED; }

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
