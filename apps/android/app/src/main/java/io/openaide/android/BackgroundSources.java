package io.openaide.android;

/**
 * The rules for the two places the background service watches Tasks (ADR-0070).
 *
 * The workspace in use is always watched: this phone's App Server, or the paired
 * computer through {@link RemoteAttention}. While the paired computer is in use the
 * service also watches this phone's App Server, but only while it is already running,
 * so the phone's own Tasks keep it awake and still notify. Each source keeps its own
 * notifications, so one source's list never withdraws the other's, and a tap on a
 * phone Task while the paired computer is in use moves to the phone before it opens
 * the Task. Kept free of Android so the rules can be tested.
 */
final class BackgroundSources {
    /** Which source a Task notification came from, carried in its tap intent. */
    static final String EXTRA_TASK_SOURCE = "task_source";

    enum Source {
        THIS_DEVICE("this_device", "task:"),
        PAIRED("paired", "paired-task:");

        /** The value carried in intents and diagnostics. */
        final String wire;
        /** The notification tag prefix; each source withdraws only its own notifications. */
        final String tag;

        Source(String wire, String tag) {
            this.wire = wire;
            this.tag = tag;
        }

        /** A distinct request code per source and Task keeps each notification's own target. */
        int requestCode(String taskId) {
            return this == THIS_DEVICE ? taskId.hashCode() : (wire + ":" + taskId).hashCode();
        }
    }

    /** What the service does after one read of this phone's App Server. */
    enum Next { RESCHEDULE, STOP_IDLE, STOP_LOST, END_WATCH }

    /** What a tap on a Task notification does. */
    enum Tap { OPEN, SWITCH_THEN_OPEN }

    private BackgroundSources() { }

    /**
     * The step after a read of this phone's App Server. With the phone in use the
     * service ends with its work, or when the server stays unreachable. With the
     * paired computer in use that computer's watch keeps the service, so the phone's
     * watch only ends once its server is gone, and a failed read is retried.
     */
    static Next afterLocalRead(boolean paired, boolean running, ServerStatus result, boolean expired, boolean visible, long now, long settlingUntil) {
        if (paired) return running ? Next.RESCHEDULE : Next.END_WATCH;
        if (expired) return Next.STOP_LOST;
        if (!visible && result != null && result.active == 0 && now >= settlingUntil) return Next.STOP_IDLE;
        return Next.RESCHEDULE;
    }

    /** When to read this phone's App Server again; idle phone Tasks are read seldom while the paired computer is in use. */
    static long delay(boolean paired, ServerStatus result, boolean visible) {
        if (result == null) return 10_000;
        if (result.active > 0) return 5_000;
        if (paired) return 30_000;
        return visible ? 15_000 : 5_000;
    }

    /** Whether the phone stays awake: only for its own working Tasks; a paired computer does its work itself. */
    static boolean protect(boolean paired, boolean watchingLocal, PowerPolicy policy, long now, long settlingUntil) {
        if (!paired) return policy.protect(now, settlingUntil);
        // Leaving the screen gives no settling time here: the read that follows decides.
        return watchingLocal && policy.protect(now, 0);
    }

    /** Only the workspace in use is on the screen; the other source's events are never in sight. */
    static boolean inUse(Source source, boolean paired) {
        return (source == Source.PAIRED) == paired;
    }

    /** A phone Task tapped while the paired computer is in use opens once the phone is in use. */
    static Tap tap(String source, boolean pairedInUse) {
        return pairedInUse && Source.THIS_DEVICE.wire.equals(source) ? Tap.SWITCH_THEN_OPEN : Tap.OPEN;
    }
}
