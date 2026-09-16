package io.openaide.android;

import org.junit.Test;
import static org.junit.Assert.*;

public class SetupPagePolicyTest {
    @Test public void connectionCommandsRequireTheOwnedSettingsSubmenu() {
        ConnectionProfile profile = new ConnectionProfile("http://127.0.0.1:5474/", "android", "test", true);
        assertTrue(SetupPagePolicy.isConnectionRoute(profile, profile.endpoint + "settings?tab=connection"));
        assertTrue(SetupPagePolicy.isConnectionRoute(profile, profile.endpoint + "settings/?tab=connection&source=menu"));
        for (String address : new String[]{"https://evil.example/settings?tab=connection", profile.endpoint + "settings",
            profile.endpoint + "tasks?tab=connection", profile.endpoint + "settings?tab=common", profile.endpoint + "settings?tab=connection&tab=connection",
            profile.endpoint + "settings?tab=connection&tab=common", profile.endpoint + "settings?tab=%", profile.endpoint + "settings?tab=connection-other"})
            assertFalse(SetupPagePolicy.isConnectionRoute(profile, address));
    }

    @Test public void servesOnlyBundledSetupAssets() {
        assertEquals("setup/index.html", SetupPagePolicy.asset("https://app.openaide.invalid/index.html"));
        assertEquals("setup/app.js", SetupPagePolicy.asset("https://app.openaide.invalid/app.js"));
        for (String address : new String[]{"https://evil.example/index.html", "file:///etc/passwd", "content://provider/file",
            "https://app.openaide.invalid/../start-termux.sh", "https://app.openaide.invalid/app.js?injected=1", "https://app.openaide.invalid:443/index.html"})
            assertNull(SetupPagePolicy.asset(address));
    }

    @Test public void onlyUserActivatedOwnedMainFrameCanOpenSettings() {
        ConnectionProfile profile = new ConnectionProfile("http://127.0.0.1:5474/", "android", "test", true);
        String source = profile.endpoint + "settings";
        String destination = "openaide://connection-settings";
        assertTrue(SetupPagePolicy.opensSettings(profile, source, destination, true, true));
        assertFalse(SetupPagePolicy.opensSettings(profile, source, destination, false, true));
        assertFalse(SetupPagePolicy.opensSettings(profile, source, destination, true, false));
        assertFalse(SetupPagePolicy.opensSettings(profile, "https://evil.example", destination, true, true));
        assertFalse(SetupPagePolicy.opensSettings(profile, source, destination + "?action=install", true, true));
    }
}
