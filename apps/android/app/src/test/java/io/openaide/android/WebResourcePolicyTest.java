package io.openaide.android;

import org.junit.Test;
import static org.junit.Assert.*;

public class WebResourcePolicyTest {
    @Test public void grantsOnlyTheSelectedDocumentUntilBrowserDisposal() {
        WebResourcePolicy policy = new WebResourcePolicy();
        String selected = "content://documents/images/123";
        assertFalse(policy.allows(selected));
        policy.allowDocument(selected);
        assertTrue(policy.allows(selected));
        assertFalse(policy.allows("content://documents/images/124"));
        policy.clear();
        assertFalse(policy.allows(selected));
    }

    @Test public void keepsPrivateFilesAndExternalOriginsBlocked() {
        WebResourcePolicy policy = new WebResourcePolicy();
        assertTrue(policy.allows("http://127.0.0.1:5474/assets/app.js"));
        assertTrue(policy.allows("blob:http://127.0.0.1:5474/preview"));
        assertFalse(policy.allows("http://127.0.0.1:5475/"));
        assertFalse(policy.allows("https://example.com/"));
        assertFalse(policy.allows("file:///data/private"));
        assertFalse(policy.allows("http://127.0.0.1:5474@evil.example/"));
    }
}
