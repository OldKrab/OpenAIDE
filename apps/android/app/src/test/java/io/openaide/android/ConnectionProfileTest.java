package io.openaide.android;

import org.junit.Test;
import static org.junit.Assert.*;

public class ConnectionProfileTest {
    @Test public void remoteResourcesStayOnTheirExactSecureOrigin() {
        ConnectionProfile profile = new ConnectionProfile("https://server.example", "user", "secret", false);
        assertTrue(profile.owns("https://server.example:443/assets/app.js"));
        assertFalse(profile.owns("https://server.example:444/"));
        assertFalse(profile.owns("https://server.example.evil/"));
        assertFalse(profile.owns("http://server.example/"));
        assertFalse(profile.owns("https://user@server.example/"));
        WebResourcePolicy policy = new WebResourcePolicy();
        policy.use(profile);
        assertFalse(policy.allows("http://127.0.0.1:5474/"));
        assertTrue(policy.allows("https://server.example/assets/app.js"));
    }
    @Test public void rejectsUnsafeRemoteProfiles() {
        for (String address : new String[]{"http://example.com", "https://user@example.com", "https://example.com/path", "https://example.com?token=secret"}) {
            assertThrows(IllegalArgumentException.class, () -> new ConnectionProfile(address, "user", "secret", false));
        }
    }
}
