package io.openaide.android;

import android.app.job.JobInfo;
import android.app.job.JobParameters;
import android.app.job.JobScheduler;
import android.app.job.JobService;
import android.content.ComponentName;
import android.content.Context;

/**
 * The daily check for a newer release while the app is closed. It only notifies;
 * downloading and installing wait for the user's tap in Settings.
 */
public final class AppUpdateJob extends JobService {
    private static final int JOB = 1;
    private static final long PERIOD_MS = 24L * 60 * 60 * 1000;

    /** Keeps one persisted, network-bound daily job; scheduling again leaves an existing one in place. */
    static void schedule(Context context) {
        JobScheduler scheduler = context.getSystemService(JobScheduler.class);
        if (scheduler.getPendingJob(JOB) != null) return;
        int result = scheduler.schedule(new JobInfo.Builder(JOB, new ComponentName(context, AppUpdateJob.class))
            .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
            .setPeriodic(PERIOD_MS)
            .setPersisted(true)
            .build());
        android.util.Log.i("OpenAIDE", "app_update_schedule outcome=" + (result == JobScheduler.RESULT_SUCCESS ? "scheduled" : "failed"));
    }

    @Override public boolean onStartJob(JobParameters params) {
        AppUpdates.get(this).check("scheduled", () -> jobFinished(params, false));
        return true;
    }

    @Override public boolean onStopJob(JobParameters params) {
        // The check is a single small request; the next period retries it.
        return false;
    }
}
