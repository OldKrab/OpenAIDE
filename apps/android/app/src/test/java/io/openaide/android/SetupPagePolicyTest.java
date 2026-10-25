package io.openaide.android;

import org.junit.Test;
import static org.junit.Assert.*;

public class SetupPagePolicyTest {
    @Test public void connectionCommandsRequireTheOwnedSettingsSubmenu() {
        ConnectionProfile profile = ConnectionProfile.local();
        assertTrue(SetupPagePolicy.isConnectionRoute(profile, profile.endpoint + "settings?tab=remote"));
        assertTrue(SetupPagePolicy.isConnectionRoute(profile, profile.endpoint + "settings?tab=general"));
        assertTrue(SetupPagePolicy.isConnectionRoute(profile, profile.endpoint + "settings/?tab=troubleshooting&source=menu"));
        for (String address : new String[]{"https://evil.example/settings?tab=remote", profile.endpoint + "settings",
            profile.endpoint + "tasks?tab=remote", profile.endpoint + "settings?tab=agents", profile.endpoint + "settings?tab=remote&tab=remote",
            profile.endpoint + "settings?tab=remote&tab=agents", profile.endpoint + "settings?tab=%", profile.endpoint + "settings?tab=remote-other"})
            assertFalse(SetupPagePolicy.isConnectionRoute(profile, address));
    }

    @Test public void onlySwitchingComputersWorksFromEveryPage() {
        assertTrue(SetupPagePolicy.allowedOnEveryPage("use_computer"));
        assertTrue(SetupPagePolicy.allowedOnEveryPage("local"));
        assertTrue(SetupPagePolicy.allowedOnEveryPage("cancel_switch"));
        for (String action : new String[]{"state", "forget_computer", "pair_setup", "install", "grant", "background", "diagnostics", ""})
            assertFalse(SetupPagePolicy.allowedOnEveryPage(action));
    }

    @Test public void onlyFixedCommandWordsAreLogged() {
        assertEquals("use_computer", SetupPagePolicy.safeAction("use_computer"));
        for (String action : new String[]{null, "", "Use computer", "use-computer", "x".repeat(41), "id=secret"})
            assertEquals("other", SetupPagePolicy.safeAction(action));
    }

    @Test public void onlySwitchCommandsAreAnsweredWhenTheSwitchEnds() {
        for (String action : new String[]{"use_computer", "local", "cancel_switch", "forget_computer"}) assertTrue(SetupPagePolicy.switchesComputer(action));
        for (String action : new String[]{"state", "pair_setup", "update_check", ""}) assertFalse(SetupPagePolicy.switchesComputer(action));
    }

    @Test public void servesOnlyBundledSetupAssets() {
        assertEquals("setup/index.html", SetupPagePolicy.asset("https://app.openaide.invalid/index.html"));
        assertEquals("setup/app.js", SetupPagePolicy.asset("https://app.openaide.invalid/app.js"));
        for (String address : new String[]{"https://evil.example/index.html", "file:///etc/passwd", "content://provider/file",
            "https://app.openaide.invalid/../start-termux.sh", "https://app.openaide.invalid/app.js?injected=1", "https://app.openaide.invalid:443/index.html"})
            assertNull(SetupPagePolicy.asset(address));
    }

    @Test public void onlyUserActivatedOwnedMainFrameCanOpenSettings() {
        ConnectionProfile profile = ConnectionProfile.local();
        String source = profile.endpoint + "settings";
        String destination = "openaide://connection-settings";
        assertTrue(SetupPagePolicy.opensSettings(profile, source, destination, true, true));
        assertFalse(SetupPagePolicy.opensSettings(profile, source, destination, false, true));
        assertFalse(SetupPagePolicy.opensSettings(profile, source, destination, true, false));
        assertFalse(SetupPagePolicy.opensSettings(profile, "https://evil.example", destination, true, true));
        assertFalse(SetupPagePolicy.opensSettings(profile, source, destination + "?action=install", true, true));
    }
}
