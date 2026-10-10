package io.openaide.android;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertThrows;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

public class ConnectionProfileTest {
    @Test public void pairedResourcesStayOnTheGatewayOrigin() {
        ConnectionProfile profile = ConnectionProfile.paired();
        assertFalse(profile.local);
        assertTrue(profile.owns("http://127.0.0.1:5475/assets/index.js"));
        assertFalse(profile.owns("http://127.0.0.1:5474/"));
        assertFalse(profile.owns("http://localhost:5475/"));
        assertFalse(profile.owns("https://127.0.0.1:5475/"));
        assertFalse(profile.owns("http://user@127.0.0.1:5475/"));
        WebResourcePolicy policy = new WebResourcePolicy();
        policy.use(profile);
        assertFalse(policy.allows("http://127.0.0.1:5474/"));
        assertFalse(policy.allows("https://example.com/"));
        assertTrue(policy.allows("http://127.0.0.1:5475/assets/index.js"));
    }

    @Test public void rejectsAnyOtherWorkspaceAddress() {
        for (String address : new String[]{"https://example.com/", "http://127.0.0.1:5475/", "http://127.0.0.1:5474/path"}) {
            assertThrows(IllegalArgumentException.class, () -> new ConnectionProfile(address, "android", "secret", true));
        }
        assertThrows(IllegalArgumentException.class, () -> new ConnectionProfile("https://example.com/", "", "", false));
        assertThrows(IllegalArgumentException.class, () -> new ConnectionProfile(ConnectionProfile.LOCAL_ENDPOINT, "android", "", true));
    }
}
