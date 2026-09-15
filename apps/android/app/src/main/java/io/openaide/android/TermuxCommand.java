package io.openaide.android;

import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.util.Log;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.BiConsumer;

public final class TermuxCommand extends BroadcastReceiver {
    private static final AtomicInteger sequence = new AtomicInteger(new java.security.SecureRandom().nextInt(Integer.MAX_VALUE));
    private static final ConcurrentHashMap<Integer, BiConsumer<Boolean, String>> callbacks = new ConcurrentHashMap<>();

    static void run(Context context, String asset, String environment, boolean persistent, BiConsumer<Boolean, String> callback) {
        int operation = sequence.incrementAndGet();
        long started = SystemClock.elapsedRealtime();
        Log.i("OpenAIDE", "termux_command_start operation=" + operation);
        callbacks.put(operation, (success, output) -> {
            Log.i("OpenAIDE", "termux_command_end operation=" + operation + " success=" + success
                + " duration_ms=" + (SystemClock.elapsedRealtime() - started));
            callback.accept(success, output);
        });
        new Handler(Looper.getMainLooper()).postDelayed(() -> finish(operation, false,
            "Termux did not return a result. Check command access and allow-external-apps=true."),
            "install-termux.sh".equals(asset) ? 180_000 : 30_000);
        try {
            String script;
            try (var input = context.getAssets().open(asset); var output = new java.io.ByteArrayOutputStream()) {
                byte[] buffer = new byte[4096];
                int count;
                while ((count = input.read(buffer)) != -1) output.write(buffer, 0, count);
                script = output.toString(StandardCharsets.UTF_8.name());
            }
            Intent result = new Intent(context, TermuxCommand.class).setAction("openaide.termux." + operation)
                .putExtra("operation", operation);
            PendingIntent pending = PendingIntent.getBroadcast(context, operation, result,
                PendingIntent.FLAG_ONE_SHOT | PendingIntent.FLAG_MUTABLE);
            Intent command = new Intent("com.termux.RUN_COMMAND").setClassName("com.termux", "com.termux.app.RunCommandService");
            command.putExtra("com.termux.RUN_COMMAND_PATH", "/data/data/com.termux/files/usr/bin/bash");
            command.putExtra("com.termux.RUN_COMMAND_ARGUMENTS", new String[]{"-s"});
            String input = environment + "\n" + script;
            if (persistent) input = environment + "\numask 077\nmkdir -p \"$HOME/.local/share/openaide-android/state\"\n"
                + "cat > \"$HOME/.local/share/openaide-android/state/start.sh\" <<'OPENAIDE_START_SCRIPT'\n"
                + script + "\nOPENAIDE_START_SCRIPT\nexec bash \"$HOME/.local/share/openaide-android/state/start.sh\"\n";
            command.putExtra("com.termux.RUN_COMMAND_STDIN", input);
            command.putExtra("com.termux.RUN_COMMAND_BACKGROUND", true);
            command.putExtra("com.termux.RUN_COMMAND_PENDING_INTENT", pending);
            context.startService(command);
            if (persistent) finish(operation, true, "started");
        } catch (Exception error) { finish(operation, false, "Cannot start Termux. Check installation and command permission."); }
    }

    static String variable(String name, String value) { return "export " + name + "='" + value.replace("'", "'\\''") + "'\n"; }

    @Override public void onReceive(Context context, Intent intent) {
        int operation = intent.getIntExtra("operation", -1);
        Bundle result = intent.getBundleExtra("result");
        boolean success = result != null && result.getInt("err", 0) == android.app.Activity.RESULT_OK && result.getInt("exitCode", -1) == 0;
        finish(operation, success, success ? result.getString("stdout", "")
            : "Termux command failed. Run setup checks; verify external-command access, packages and free storage.");
    }

    private static void finish(int operation, boolean success, String output) {
        BiConsumer<Boolean, String> callback = callbacks.remove(operation);
        if (callback != null) callback.accept(success, output);
    }
}
