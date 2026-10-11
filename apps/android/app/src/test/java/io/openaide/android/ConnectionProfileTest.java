package io.openaide.android;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

public class ConnectionProfileTest {
    @Test public void pairedResourcesStayOnTheGatewayOrigin() {
        ConnectionProfile profile = ConnectionProfile.paired();
        assertFalse(profile.local);
        assertTrue(profile.owns("http://127.0.0.1:5475/assets/index.js"));
        assertFalse(profile.owns("http://127.0.0.1:5476/"));
        assertFalse(profile.owns("http://localhost:5475/"));
        assertFalse(profile.owns("https://127.0.0.1:5475/"));
        assertFalse(profile.owns("http://user@127.0.0.1:5475/"));
        WebResourcePolicy policy = new WebResourcePolicy();
        policy.use(profile);
        assertFalse(policy.allows("http://127.0.0.1:5476/"));
        assertFalse(policy.allows("https://example.com/"));
        assertTrue(policy.allows("http://127.0.0.1:5475/assets/index.js"));
    }

    @Test public void eachWorkspaceKeepsItsOwnOrigin() {
        ConnectionProfile profile = ConnectionProfile.local();
        assertTrue(profile.local);
        assertTrue(profile.owns("http://127.0.0.1:5476/settings?tab=connection"));
        // The paired computer's stored drafts and session must not be readable from the local workspace.
        assertFalse(profile.owns("http://127.0.0.1:5475/"));
        // The Web Shell that Termux used to serve is no longer an origin of the app.
        assertFalse(profile.owns("http://127.0.0.1:5474/"));
    }
}
