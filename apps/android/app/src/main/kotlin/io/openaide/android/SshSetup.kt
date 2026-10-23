package io.openaide.android

import android.app.Activity
import android.app.AlertDialog
import android.content.ClipData
import android.content.ClipboardManager
import android.graphics.Typeface
import android.os.SystemClock
import android.text.InputType
import android.view.View
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import io.openaide.android.ssh.SshException
import io.openaide.android.ssh.authorizeKey
import io.openaide.android.ssh.hostKeyFingerprint
import io.openaide.android.ssh.publicKeyLine
import io.openaide.android.ssh.readHostKey
import java.security.SecureRandom
import java.util.concurrent.Executors

/**
 * Adds an SSH Computer with native dialogs, so it works with or without a workspace
 * open (ADR-0069): the address, then the computer's host key for the user to accept,
 * then this phone's public key to add there, either by the user or with a password
 * used once. The accepted host key is pinned; the password is never kept.
 */
internal class SshSetup private constructor(private val activity: Activity, private val added: Runnable) {
    private val diagnostics = AndroidDiagnostics()
    private val started = SystemClock.elapsedRealtime()
    private var destination = ""
    private var name = ""

    companion object {
        private val worker = Executors.newSingleThreadExecutor { task -> Thread(task, "openaide-ssh-setup").apply { isDaemon = true } }
        private const val KEY_COMMENT = "openaide-android"

        /** Starts the flow; [added] runs once the computer is saved and selected. */
        @JvmStatic fun start(activity: Activity, added: Runnable) {
            SshSetup(activity, added).also {
                it.diagnostics.record("ssh_add", "started", 0, 0)
                it.askAddress(null)
            }
        }
    }

    private fun askAddress(error: String?) {
        val form = form()
        form.addView(text("OpenAIDE installs itself on the Linux computer over SSH and keeps running there while agents work."))
        val host = field(form, "Host", "user@host or user@host:port", destination)
        val label = field(form, "Name (optional)", "Build server", name)
        if (error != null) host.error = error
        dialog("Add an SSH computer", form, "Next", { dialog ->
            destination = host.text.toString().trim()
            name = label.text.toString()
            val parsed = SshComputers.parseDestination(destination, null)
            if (parsed == null) { host.error = "Enter the user and the computer, such as dev@192.168.1.20"; return@dialog }
            dialog.dismiss()
            readKey(parsed)
        })
    }

    private fun readKey(parsed: Array<String>) {
        val progress = progress("Connecting to the computer…")
        worker.execute {
            val target = SshServer.target(computer("", parsed, ""))
            val result = try {
                val key = readHostKey(target)
                Result.success(key to hostKeyFingerprint(key))
            } catch (error: SshException.Failed) { Result.failure(error) }
            ui {
                progress.dismiss()
                result.fold({ (key, fingerprint) -> confirmHostKey(parsed, key, fingerprint) }, { failure ->
                    finish("host_key_${(failure as SshException.Failed).`class`}")
                    askAddress(SshServer.message(failure.`class`))
                })
            }
        }
    }

    private fun confirmHostKey(parsed: Array<String>, key: String, fingerprint: String) {
        val form = form()
        form.addView(text("OpenAIDE has not connected to ${parsed[0]} before. Check that the computer shows this fingerprint for its host key:"))
        form.addView(mono(fingerprint))
        form.addView(text("On the computer, ssh-keygen -lf /etc/ssh/ssh_host_*_key.pub prints it. If the key ever changes, OpenAIDE will not connect."))
        dialog("Is this the right computer?", form, "Trust", { dialog ->
            dialog.dismiss()
            diagnostics.record("ssh_add_host_key", "accepted", 0, SystemClock.elapsedRealtime() - started)
            authorize(parsed, key)
        }, onCancel = { finish("host_key_declined") })
    }

    private fun authorize(parsed: Array<String>, hostKey: String) {
        val store = ConnectionStore(activity)
        val line = try { publicKeyLine(store.sshKey(), KEY_COMMENT) } catch (error: Exception) { null }
        if (line == null) { finish("key_unavailable"); notice("This phone could not create its SSH key. Restart OpenAIDE and try again."); return }
        val form = form()
        form.addView(text("Let this phone sign in to ${parsed[2]} by adding its key to ~/.ssh/authorized_keys there:"))
        val key = mono(line)
        form.addView(key)
        form.addView(text("Or enter the password once and OpenAIDE adds the key. The password is not kept."))
        val password = field(form, "Password (optional)", "", "")
        password.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
        // A password input switches to monospace; keep the hint in the dialog's typeface.
        password.typeface = Typeface.DEFAULT
        dialog("Let this phone sign in", form, "Connect", { dialog ->
            val secret = password.text.toString()
            password.setText("")
            if (secret.isEmpty()) { dialog.dismiss(); save(parsed, hostKey); return@dialog }
            dialog.dismiss()
            val progress = progress("Adding this phone’s key…")
            worker.execute {
                val failure = try {
                    authorizeKey(SshServer.target(computer("", parsed, hostKey)), hostKey, secret, line, SshServer.events(activity))
                    null
                } catch (error: SshException.Failed) { error.`class` }
                ui {
                    progress.dismiss()
                    diagnostics.record("ssh_add_authorize", failure ?: "success", 1, SystemClock.elapsedRealtime() - started)
                    if (failure == null) save(parsed, hostKey) else { finish("authorize_$failure"); notice(authorizeMessage(failure)) }
                }
            }
        }, neutral = "Copy key" to {
            activity.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("OpenAIDE SSH key", line))
            diagnostics.record("ssh_add_key", "copied", 0, 0)
        }, onCancel = { finish("key_declined") })
    }

    private fun save(parsed: Array<String>, hostKey: String) {
        val id = ByteArray(16).also(SecureRandom()::nextBytes).joinToString("") { "%02x".format(it.toInt() and 0xff) }
        val store = ConnectionStore(activity)
        store.saveSsh(computer(id, parsed, hostKey))
        // A connection to the computer used until now must not outlive the choice.
        RemoteNode.disconnect()
        SshServer.reset()
        finish("added")
        added.run()
    }

    private fun computer(id: String, parsed: Array<String>, hostKey: String) = SshComputers.Computer(
        id, SshComputers.name(name).ifEmpty { parsed[0] }, parsed[0], parsed[1].toInt(), parsed[2], hostKey)

    private fun authorizeMessage(reason: String) = when (reason) {
        "auth_failed" -> "The computer did not accept that password."
        "authorized_keys_unwritable" -> "OpenAIDE signed in but could not add the key to ~/.ssh/authorized_keys. Add it there yourself, then connect."
        else -> SshServer.message(reason)
    }

    private fun finish(outcome: String) = diagnostics.record("ssh_add", outcome, 0, SystemClock.elapsedRealtime() - started)

    private fun ui(action: () -> Unit) = activity.runOnUiThread { if (!activity.isDestroyed) action() }

    private fun notice(message: String) {
        if (activity.isDestroyed) return
        AlertDialog.Builder(activity).setTitle("SSH computer not added").setMessage(message).setPositiveButton("OK", null).show()
    }

    private fun progress(message: String): AlertDialog =
        AlertDialog.Builder(activity).setMessage(message).setCancelable(false).show()

    /** A dialog whose positive button may keep it open to show an error in the form. */
    private fun dialog(title: String, content: View, positive: String, onPositive: (AlertDialog) -> Unit,
                       neutral: Pair<String, () -> Unit>? = null, onCancel: () -> Unit = { finish("cancelled") }) {
        if (activity.isDestroyed) return
        val builder = AlertDialog.Builder(activity).setTitle(title).setView(content)
            .setPositiveButton(positive, null).setNegativeButton("Cancel") { _, _ -> onCancel() }
            .setOnCancelListener { onCancel() }
        if (neutral != null) builder.setNeutralButton(neutral.first, null)
        val dialog = builder.create()
        dialog.setOnShowListener {
            dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener { onPositive(dialog) }
            if (neutral != null) dialog.getButton(AlertDialog.BUTTON_NEUTRAL).setOnClickListener { neutral.second() }
        }
        dialog.show()
    }

    private fun form() = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL
        val padding = (20 * resources.displayMetrics.density).toInt()
        setPadding(padding, padding / 2, padding, 0)
    }

    /** Adds a labelled single-line input to [form]; the label stays visible once the user types. */
    private fun field(form: LinearLayout, label: String, hint: String, value: String): EditText {
        val caption = TextView(activity).apply {
            text = label
            setPadding(0, 16, 0, 0)
        }
        val input = EditText(activity).apply {
            this.hint = hint
            contentDescription = label
            setText(value)
            isSingleLine = true
            importantForAutofill = View.IMPORTANT_FOR_AUTOFILL_NO
        }
        caption.labelFor = View.generateViewId().also { input.id = it }
        form.addView(caption)
        form.addView(input)
        return input
    }

    private fun text(value: String) = TextView(activity).apply {
        text = value
        setPadding(0, 8, 0, 8)
    }

    private fun mono(value: String) = TextView(activity).apply {
        text = value
        typeface = Typeface.MONOSPACE
        setTextIsSelectable(true)
        setPadding(0, 8, 0, 8)
    }
}
