package io.openaide.android;

import org.junit.Test;
import static org.junit.Assert.*;

public class PowerPolicyTest {
    @Test public void idleDoesNotHoldThePhoneAwake() {
        PowerPolicy policy = new PowerPolicy(0);
        policy.observe(0, 100);
        assertFalse(policy.protect(100, 0));
    }
    @Test public void workRemainsProtectedAcrossLockAndTemporaryDisconnect() {
        PowerPolicy policy = new PowerPolicy(0);
        policy.observe(1, 100);
        assertTrue(policy.protect(60_000, 0));
        policy.observe(1, 60_000);
        assertTrue(policy.protect(160_000, 0));
        policy.observe(0, 170_000);
        assertFalse(policy.protect(170_000, 0));
    }
    @Test public void unknownStateHasABoundedRecoveryGrace() {
        PowerPolicy policy = new PowerPolicy(0);
        policy.observe(1, 0);
        assertFalse(policy.protect(120_001, 0));
        assertTrue(policy.expired(120_001));
        assertTrue(policy.protect(120_001, 130_000));
    }
}
