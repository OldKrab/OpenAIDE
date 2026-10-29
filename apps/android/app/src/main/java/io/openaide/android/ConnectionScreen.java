package io.openaide.android;

import android.app.Activity;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.net.ConnectivityManager;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.text.format.DateUtils;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;

/**
 * The native screen shown while the workspace is not open: its status, "Try again",
 * and, after a failure, what the user can do instead (ADR-0070): when this computer
 * last worked, and a direct switch to every other computer.
 *
 * Owned by one activity and used on the main thread only. It decides nothing about
 * connecting: the activity connects and reports progress and failures here, and a
 * switch is made by a {@link ComputerSwitch}, which saves the choice only once the
 * target is connected. The activity is told when the selection changed, so it can
 * reload the profile and reconnect.
 */
final class ConnectionScreen {
    /** What the activity does for the screen's buttons. */
    interface Host {
        /** "Open workspace" and "Try again". */
        void openWorkspace();
        void openConnectionSettings();
        /** A switch took effect: reload the profile and connect to the new computer. */
        void selectionChanged();
    }

    /** Under a minute the relative time would read "0 minutes ago". */
    private static final long JUST_NOW_MS = 60_000;

    private final Activity activity;
    private final ConnectionStore store;
    private final AndroidDiagnostics diagnostics;
    private final Host host;
    private final ComputerSwitch<?> switcher;
    private TextView status;
    private TextView lastConnected;
    private Button connect;
    private Button setup;
    private Button cancel;
    private LinearLayout choices;
    /** The failure sentence on screen, restored when a switch is cancelled. */
    private String failure = "";

    ConnectionScreen(Activity activity, ConnectionStore store, AndroidDiagnostics diagnostics, Host host) {
        this.activity = activity;
        this.store = store;
        this.diagnostics = diagnostics;
        this.host = host;
        switcher = RemotePairing.computerSwitch(activity, () -> { if (!activity.isDestroyed()) host.selectionChanged(); });
    }

    /** Builds the screen afresh and shows it; a switch still connecting for the previous one is cancelled. */
    void show() {
        if (switcher.getSwitching()) switcher.cancel();
        LinearLayout layout = new LinearLayout(activity);
        layout.setOrientation(LinearLayout.VERTICAL);
        layout.setGravity(Gravity.CENTER);
        int padding = dp(24);
        layout.setPadding(padding, padding * 2, padding, padding);
        layout.setBackgroundColor(ShellColors.background(activity));
        ImageView icon = new ImageView(activity);
        icon.setImageResource(R.mipmap.ic_launcher);
        icon.setContentDescription("OpenAIDE");
        layout.addView(icon, centered(dp(72), dp(72), 0));
        TextView title = new TextView(activity);
        title.setText("OpenAIDE");
        title.setTextSize(24);
        title.setTypeface(Typeface.create("sans-serif-medium", Typeface.NORMAL));
        title.setTextColor(ShellColors.text(activity));
        title.setGravity(Gravity.CENTER);
        layout.addView(title, centered(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, 16));
        TextView instructions = new TextView(activity);
        instructions.setText("Your agent workspace");
        instructions.setGravity(Gravity.CENTER);
        instructions.setTextSize(15);
        instructions.setTextColor(ShellColors.muted(activity));
        layout.addView(instructions, centered(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, 4));
        status = new TextView(activity);
        status.setTextSize(15);
        status.setGravity(Gravity.CENTER);
        status.setTextColor(ShellColors.muted(activity));
        layout.addView(status, centered(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, 32));
        lastConnected = new TextView(activity);
        lastConnected.setTextSize(13);
        lastConnected.setGravity(Gravity.CENTER);
        lastConnected.setTextColor(ShellColors.muted(activity));
        lastConnected.setVisibility(View.GONE);
        layout.addView(lastConnected, centered(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, 6));
        connect = new Button(activity);
        connect.setText("Open workspace");
        connect.setAllCaps(false);
        connect.setTextSize(15);
        connect.setTypeface(Typeface.create("sans-serif-medium", Typeface.NORMAL));
        connect.setTextColor(ShellColors.background(activity));
        connect.setStateListAnimator(null);
        GradientDrawable fill = new GradientDrawable();
        fill.setColor(ShellColors.text(activity));
        fill.setCornerRadius(dp(26));
        connect.setBackground(fill);
        connect.setOnClickListener(view -> host.openWorkspace());
        layout.addView(connect, centered(dp(260), dp(52), 28));
        choices = new LinearLayout(activity);
        choices.setOrientation(LinearLayout.VERTICAL);
        choices.setGravity(Gravity.CENTER_HORIZONTAL);
        choices.setVisibility(View.GONE);
        layout.addView(choices, centered(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, 0));
        cancel = secondaryButton("Cancel");
        cancel.setVisibility(View.GONE);
        cancel.setOnClickListener(view -> switcher.cancel());
        layout.addView(cancel, centered(dp(260), dp(48), 4));
        setup = secondaryButton("Connection settings");
        setup.setOnClickListener(view -> host.openConnectionSettings());
        layout.addView(setup, centered(dp(260), dp(48), 4));
        ScrollView scroll = new ScrollView(activity);
        scroll.setFillViewport(true);
        scroll.addView(layout);
        scroll.setOnApplyWindowInsetsListener((view, insets) -> {
            view.setPadding(insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(),
                insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom());
            return insets;
        });
        activity.setContentView(scroll);
    }

    /** Ends a switch still connecting, as when the activity goes away; the selection stays as it was. */
    void dispose() {
        if (switcher.getSwitching()) switcher.cancel();
    }

    /** A message with nothing to press: the workspace is opening. */
    void progress(String message) {
        failure = "";
        connect.setEnabled(false);
        connect.setVisibility(View.GONE);
        choices.setVisibility(View.GONE);
        lastConnected.setVisibility(View.GONE);
        status.setText(message);
    }

    /** Replaces the sentence without changing the buttons. */
    void status(String message) { status.setText(message); }

    /**
     * A connect to the computer in use failed. Shows the sentence ([reason] is the
     * failure class, or null), when this computer last worked, and the other computers to
     * work. Returns the sentence shown, which says "offline" instead of "can't reach"
     * when the phone has no working internet.
     */
    String failed(String message, ConnectionProfile.Kind kind, String reason, int attempt) {
        ConnectionFailure.Classification classification = ConnectionFailure.classify(kind, reason, online());
        failure = ConnectionFailure.message(classification, message);
        List<SwitchChoices.Choice> others = SwitchChoices.list(ConnectionController.thisDeviceReady(activity, store),
            store.pairedServers(), store.sshComputers(), store.inUseTarget());
        Map<String, Object> fields = new HashMap<>();
        fields.put("profile_kind", kind.name().toLowerCase(Locale.ROOT));
        if (reason != null) fields.put("error_class", reason);
        fields.put("switch_choices", others.size());
        diagnostics.record("connection_failure_shown", classification.word, attempt, 0, fields);
        connect.setText("Try again");
        buildChoices(others);
        showFailure();
        return failure;
    }

    private void showFailure() {
        connect.setEnabled(true);
        connect.setVisibility(View.VISIBLE);
        setup.setVisibility(View.VISIBLE);
        cancel.setVisibility(View.GONE);
        status.setText(failure);
        long connectedAt = store.lastConnected(store.inUseTarget());
        if (connectedAt > 0) {
            long now = System.currentTimeMillis();
            CharSequence ago = now - connectedAt < JUST_NOW_MS ? "just now"
                : DateUtils.getRelativeTimeSpanString(connectedAt, now, DateUtils.MINUTE_IN_MILLIS);
            lastConnected.setText(ConnectionFailure.lastConnectedLine(ago.toString()));
            lastConnected.setVisibility(View.VISIBLE);
        } else lastConnected.setVisibility(View.GONE);
        choices.setVisibility(choices.getChildCount() > 0 ? View.VISIBLE : View.GONE);
    }

    private void buildChoices(List<SwitchChoices.Choice> others) {
        choices.removeAllViews();
        for (SwitchChoices.Choice choice : others) {
            Button button = secondaryButton(choice.label());
            button.setOnClickListener(view -> switchTo(choice));
            choices.addView(button, centered(dp(260), dp(48), 4));
        }
    }

    /** Connects [choice] before the selection changes; while it connects, only Cancel is offered. */
    private void switchTo(SwitchChoices.Choice choice) {
        // The controls change first: a request that is refused at once answers before it returns.
        connect.setVisibility(View.GONE);
        setup.setVisibility(View.GONE);
        choices.setVisibility(View.GONE);
        lastConnected.setVisibility(View.GONE);
        cancel.setVisibility(View.VISIBLE);
        status.setText(choice.connectingText());
        switcher.request(store.inUseTarget(), choice.target, null, (outcome, problem) -> {
            if (activity.isDestroyed()) return;
            if (problem != null) failure = problem;
            // A switch that took effect replaces this screen next; every other answer leaves the selection as it was.
            if (problem != null || !"switched".equals(outcome)) showFailure();
        });
    }

    /** Whether Android has a network it has verified reaches the internet. */
    private boolean online() {
        try {
            ConnectivityManager manager = activity.getSystemService(ConnectivityManager.class);
            Network network = manager == null ? null : manager.getActiveNetwork();
            NetworkCapabilities capabilities = network == null ? null : manager.getNetworkCapabilities(network);
            return capabilities != null && capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
                && capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED);
        } catch (RuntimeException error) {
            // Not knowing is not offline: keep the sentence that names the computer.
            return true;
        }
    }

    /** A button without a fill, like "Connection settings". */
    private Button secondaryButton(String text) {
        Button button = new Button(activity);
        button.setText(text);
        button.setAllCaps(false);
        button.setTextSize(15);
        button.setTextColor(ShellColors.text(activity));
        button.setStateListAnimator(null);
        button.setBackgroundColor(Color.TRANSPARENT);
        return button;
    }

    private int dp(int value) { return ShellColors.dp(activity, value); }

    private LinearLayout.LayoutParams centered(int width, int height, int topMarginDp) {
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(width, height);
        params.gravity = Gravity.CENTER_HORIZONTAL;
        params.topMargin = dp(topMarginDp);
        return params;
    }
}
