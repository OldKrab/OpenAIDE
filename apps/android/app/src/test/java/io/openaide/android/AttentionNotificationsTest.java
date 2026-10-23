package io.openaide.android;

import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import org.junit.Test;
import static org.junit.Assert.*;

public class AttentionNotificationsTest {
    private static final long HIDDEN_AT = 1_000_000L;

    @Test public void startupBacklogStaysSilent() {
        AttentionNotifications policy = new AttentionNotifications();
        AttentionNotifications.Decision decision = policy.observe(List.of(event("task-1", "event-1", HIDDEN_AT + 5)), false, HIDDEN_AT);

        assertTrue(decision.show.isEmpty());
        assertEquals(1, decision.baseline);
    }

    @Test public void eventAfterLeavingTheScreenNotifiesOnce() {
        AttentionNotifications policy = new AttentionNotifications();
        policy.observe(Collections.emptyList(), true, 0);
        List<AttentionNotifications.Attention> current = List.of(event("task-1", "event-1", HIDDEN_AT + 5));

        List<AttentionNotifications.Attention> shown = policy.observe(current, false, HIDDEN_AT).show;

        assertEquals(1, shown.size());
        assertEquals("task-1", shown.get(0).taskId);
        AttentionNotifications.Decision repeated = policy.observe(current, false, HIDDEN_AT);
        assertTrue(repeated.show.isEmpty());
        assertEquals(0, repeated.baseline + repeated.inSight);
    }

    @Test public void eventThatOccurredInSightStaysInProduct() {
        AttentionNotifications policy = new AttentionNotifications();
        policy.observe(Collections.emptyList(), true, 0);

        AttentionNotifications.Decision visible = policy.observe(List.of(event("task-1", "event-1", HIDDEN_AT + 5)), true, 0);
        AttentionNotifications.Decision before = policy.observe(List.of(event("task-2", "event-2", HIDDEN_AT - 5)), false, HIDDEN_AT);

        assertTrue(visible.show.isEmpty());
        assertEquals(1, visible.inSight);
        assertTrue(before.show.isEmpty());
        assertEquals(1, before.inSight);
    }

    @Test public void notificationIsWithdrawnOnceItsEventIsNoLongerCurrent() {
        Map<String, String> shown = Map.of("task-1", "event-1", "task-2", "event-2", "task-3", "event-3");
        List<String> stale = AttentionNotifications.stale(shown, Arrays.asList(
            event("task-1", "event-1", 0),
            event("task-2", "event-4", 0)));

        Collections.sort(stale);
        assertEquals(List.of("task-2", "task-3"), stale);
    }

    @Test public void readsBothTimestampFormats() {
        assertEquals(1784116599266L, AttentionNotifications.timestamp("1784116599266"));
        assertEquals(1784116599266L, AttentionNotifications.timestamp("2026-07-15T11:56:39.266Z"));
        assertEquals(Long.MIN_VALUE, AttentionNotifications.timestamp("soon"));
    }

    private static AttentionNotifications.Attention event(String taskId, String eventId, long occurredAt) {
        return new AttentionNotifications.Attention(taskId, "Fix the build", eventId, "finished", occurredAt);
    }
}
