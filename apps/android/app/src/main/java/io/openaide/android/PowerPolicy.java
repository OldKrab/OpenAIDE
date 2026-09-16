package io.openaide.android;

final class PowerPolicy {
    private long lastHealthy;
    private boolean active;
    PowerPolicy(long now) { lastHealthy = now; }
    void observe(int activeTasks, long now) { active = activeTasks > 0; lastHealthy = now; }
    boolean protect(long now, long settlingUntil) {
        return now < settlingUntil || (active && now - lastHealthy < 120_000);
    }
    boolean expired(long now) { return now - lastHealthy >= 120_000; }
}
