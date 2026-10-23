package io.openaide.android;

import java.util.Collections;
import java.util.List;
import java.util.Map;
import org.junit.Test;
import static org.junit.Assert.*;

public class BackgroundSourcesTest {
    private static final BackgroundSources.Source PHONE = BackgroundSources.Source.THIS_DEVICE;
    private static final BackgroundSources.Source PAIRED = BackgroundSources.Source.PAIRED;

    @Test public void eachSourceKeepsItsOwnNotificationsAndTargets() {
        assertNotEquals(PHONE.tag, PAIRED.tag);
        assertFalse(PHONE.tag.startsWith(PAIRED.tag) || PAIRED.tag.startsWith(PHONE.tag));
        // The phone's notifications keep the target they had before the paired source existed.
        assertEquals("task-1".hashCode(), PHONE.requestCode("task-1"));
        assertNotEquals(PHONE.requestCode("task-1"), PAIRED.requestCode("task-1"));
        // A list read from one source never withdraws the other's notification of the same Task id.
        Map<String, String> shownByPhone = Map.of("task-1", "phone-event");
        assertTrue(AttentionNotifications.stale(shownByPhone, List.of(event("task-1", "phone-event"))).isEmpty());
    }

    @Test public void thePhonesWatchWhilePairedStartsWithItsOwnSilentBaseline() {
        AttentionNotifications paired = new AttentionNotifications();
        AttentionNotifications phone = new AttentionNotifications();
        paired.observe(Collections.emptyList(), false, 0);

        AttentionNotifications.Decision first = phone.observe(List.of(event("task-1", "event-1")), false, 0);
        assertTrue(first.show.isEmpty());
        assertEquals(1, first.baseline);
        // The paired computer's baseline was already read, so its new event notifies.
        assertEquals(1, paired.observe(List.of(event("task-2", "event-2")), false, 0).show.size());
    }

    @Test public void onlyTheWorkspaceInUseIsOnTheScreen() {
        assertTrue(BackgroundSources.inUse(PAIRED, true));
        assertFalse(BackgroundSources.inUse(PHONE, true));
        assertTrue(BackgroundSources.inUse(PHONE, false));
        assertFalse(BackgroundSources.inUse(PAIRED, false));
    }

    @Test public void phoneTasksKeepThePhoneAwakeWhilePaired() {
        PowerPolicy policy = new PowerPolicy(0);
        policy.observe(1, 1_000);
        assertTrue(BackgroundSources.protect(true, true, policy, 2_000, 0));
        // Once the phone's watch ended, nothing on the phone works.
        assertFalse(BackgroundSources.protect(true, false, policy, 2_000, 0));
        // Leaving the screen while paired gives no settling wake lock without a working phone Task.
        policy.observe(0, 1_000);
        assertFalse(BackgroundSources.protect(true, true, policy, 2_000, 60_000));
        assertTrue(BackgroundSources.protect(false, false, policy, 2_000, 60_000));
        // A read that stops answering does not hold the phone awake for ever.
        policy.observe(1, 1_000);
        assertFalse(BackgroundSources.protect(true, true, policy, 1_000 + 120_000, 0));
    }

    @Test public void onlyThePhoneInUseStopsWhenIdleOrLost() {
        ServerStatus idle = new ServerStatus(0, 0, Collections.emptyList());
        ServerStatus working = new ServerStatus(1, 0, Collections.emptyList());
        assertEquals(BackgroundSources.Next.STOP_IDLE, BackgroundSources.afterLocalRead(false, true, idle, false, false, 20_000, 10_000));
        assertEquals(BackgroundSources.Next.RESCHEDULE, BackgroundSources.afterLocalRead(false, true, idle, false, false, 5_000, 10_000));
        assertEquals(BackgroundSources.Next.RESCHEDULE, BackgroundSources.afterLocalRead(false, true, working, false, false, 20_000, 10_000));
        assertEquals(BackgroundSources.Next.STOP_LOST, BackgroundSources.afterLocalRead(false, true, null, true, false, 20_000, 10_000));
        // While paired, that computer's watch keeps the service: an idle or failing phone does not stop it.
        assertEquals(BackgroundSources.Next.RESCHEDULE, BackgroundSources.afterLocalRead(true, true, idle, false, false, 20_000, 10_000));
        assertEquals(BackgroundSources.Next.RESCHEDULE, BackgroundSources.afterLocalRead(true, true, null, true, false, 20_000, 10_000));
        // A phone whose App Server is not running is not watched, and not started.
        assertEquals(BackgroundSources.Next.END_WATCH, BackgroundSources.afterLocalRead(true, false, null, true, false, 20_000, 10_000));
    }

    @Test public void idlePhoneTasksAreReadSeldomWhilePaired() {
        ServerStatus idle = new ServerStatus(0, 0, Collections.emptyList());
        ServerStatus working = new ServerStatus(1, 0, Collections.emptyList());
        assertEquals(30_000, BackgroundSources.delay(true, idle, false));
        assertEquals(5_000, BackgroundSources.delay(true, working, true));
        assertEquals(10_000, BackgroundSources.delay(true, null, false));
        assertEquals(15_000, BackgroundSources.delay(false, idle, true));
        assertEquals(5_000, BackgroundSources.delay(false, idle, false));
    }

    @Test public void aPhoneTaskTappedWhilePairedSwitchesToThePhoneFirst() {
        assertEquals(BackgroundSources.Tap.SWITCH_THEN_OPEN, BackgroundSources.tap("this_device", true));
        assertEquals(BackgroundSources.Tap.OPEN, BackgroundSources.tap("this_device", false));
        assertEquals(BackgroundSources.Tap.OPEN, BackgroundSources.tap("paired", true));
        // A notification from before sources were carried opens where it is.
        assertEquals(BackgroundSources.Tap.OPEN, BackgroundSources.tap(null, true));
    }

    private static AttentionNotifications.Attention event(String task, String id) {
        return new AttentionNotifications.Attention(task, "Task", id, "completed", 5);
    }
}
