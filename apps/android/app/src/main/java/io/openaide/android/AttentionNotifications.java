package io.openaide.android;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Decides which Task Attention Events become Android notifications.
 *
 * App Server owns the events; this only applies the shell rules to the task lists the
 * background service reads. The first list is a silent baseline, so a backlog never
 * turns into notifications. An event notifies once, only when it occurred after the
 * app left the screen, so one that happened in sight stays an in-product indicator.
 * Each Task has at most one notification, withdrawn once its event is no longer current;
 * that check reads the notifications Android still shows, so it survives a restarted service.
 * The WebView cannot do this itself: its JavaScript is paused while the app is hidden.
 */
final class AttentionNotifications {
    static final int MAX_HANDLED = 500;

    static final class Attention {
        final String taskId;
        final String title;
        final String eventId;
        final String reason;
        final long occurredAt;

        Attention(String taskId, String title, String eventId, String reason, long occurredAt) {
            this.taskId = taskId;
            this.title = title;
            this.eventId = eventId;
            this.reason = reason;
            this.occurredAt = occurredAt;
        }
    }

    private final Set<String> handled = new LinkedHashSet<>();
    private boolean baselineInstalled;

    /** Applies one authoritative list; `hiddenSince` is the wall-clock time the app left the screen. */
    List<Attention> observe(List<Attention> current, boolean visible, long hiddenSince) {
        List<Attention> show = new ArrayList<>();
        boolean baseline = !baselineInstalled;
        baselineInstalled = true;
        for (Attention attention : current) {
            if (!remember(attention.eventId) || baseline) continue;
            if (visible || attention.occurredAt < hiddenSince) continue;
            show.add(attention);
        }
        return show;
    }

    /** The Tasks whose shown notification, by Task id to event id, no longer matches their current event. */
    static List<String> stale(Map<String, String> shown, List<Attention> current) {
        Map<String, String> events = new HashMap<>();
        for (Attention attention : current) events.put(attention.taskId, attention.eventId);
        List<String> stale = new ArrayList<>();
        for (Map.Entry<String, String> entry : shown.entrySet()) {
            if (!entry.getValue().equals(events.get(entry.getKey()))) stale.add(entry.getKey());
        }
        return stale;
    }

    private boolean remember(String eventId) {
        if (!handled.add(eventId)) return false;
        Iterator<String> oldest = handled.iterator();
        while (handled.size() > MAX_HANDLED) { oldest.next(); oldest.remove(); }
        return true;
    }

    /** The short product reason; Chat, tool, and question content stay private. */
    static String title(String reason) {
        switch (reason) {
            case "needsPermission": return "Permission needed";
            case "needsAnswer": return "Answer needed";
            case "stopped": return "Task stopped";
            case "failed": return "Task failed";
            default: return "Task finished";
        }
    }

    /** App Server writes epoch milliseconds; older records may carry an ISO-8601 instant. */
    static long timestamp(String value) {
        String trimmed = value == null ? "" : value.trim();
        try {
            if (trimmed.matches("\\d+")) return Long.parseLong(trimmed);
            return java.time.Instant.parse(trimmed).toEpochMilli();
        } catch (RuntimeException invalid) {
            return Long.MIN_VALUE;
        }
    }
}
