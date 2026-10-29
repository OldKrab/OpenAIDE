package io.openaide.android;

import android.content.Context;
import android.content.SharedPreferences;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.util.Base64;
import java.nio.charset.StandardCharsets;
import java.security.KeyStore;
import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

final class ConnectionStore {
    private static final String RELAY_PREFIX = "server_relay_";
    private static final String LAST_CONNECTED_PREFIX = "last_connected_";
    private final SharedPreferences preferences;
    /** The computer this phone is paired with: its key and the name it reported. */
    static final class PairedServer {
        final String id;
        final String name;
        PairedServer(String id, String name) { this.id = id; this.name = name; }
    }

    ConnectionStore(Context context) {
        preferences = context.getSharedPreferences("connection", Context.MODE_PRIVATE);
        // Address-and-password connections were replaced by pairing; a phone that
        // used one starts setup again instead of failing to sign in.
        if (preferences.getBoolean("remote", false)) {
            preferences.edit().remove("remote").remove("remote_url").remove("remote_user").remove("remote_secret")
                .putBoolean("configured", false).apply();
        }
        // The Web Shell in Termux and its password are gone; the App Server's own token replaces it.
        if (preferences.contains("password")) preferences.edit().remove("password").apply();
    }

    ConnectionProfile load() {
        if (usesSsh()) return ConnectionProfile.ssh();
        return usesPaired() ? ConnectionProfile.paired() : ConnectionProfile.local();
    }

    /** Whether the paired computer, rather than this phone, is the selected workspace. */
    boolean usesPaired() { return !usesSsh() && preferences.getBoolean("paired", false) && pairedServer() != null; }

    /** Whether an SSH Computer is the selected workspace. */
    boolean usesSsh() { return sshComputer() != null; }

    /** The SSH Computer selected for work, or null. */
    SshComputers.Computer sshComputer() {
        String id = preferences.getString("ssh_id", "");
        return id.isEmpty() ? null : SshComputers.find(sshComputers(), id);
    }

    /** Every SSH Computer this phone knows, in the order they were added. */
    java.util.List<SshComputers.Computer> sshComputers() { return SshComputers.parse(preferences.getString("ssh_computers", "")); }

    /** Adding an SSH Computer remembers its accepted host key and selects it. */
    void saveSsh(SshComputers.Computer computer) {
        preferences.edit().putString("ssh_computers", SshComputers.format(SshComputers.with(sshComputers(), computer)))
            .putString("ssh_id", computer.id).putBoolean("paired", false).putBoolean("configured", true).apply();
    }

    /** Selects one of the SSH Computers; returns false for one this phone does not know. */
    boolean selectSsh(String id) {
        if (SshComputers.find(sshComputers(), id) == null) return false;
        preferences.edit().putString("ssh_id", id).putBoolean("paired", false).putBoolean("configured", true).apply();
        return true;
    }

    /** Stops knowing an SSH Computer; forgetting the selected one leaves this phone as the computer in use. */
    void forgetSsh(String id) {
        SharedPreferences.Editor edit = preferences.edit().putString("ssh_computers", SshComputers.format(SshComputers.without(sshComputers(), id)));
        if (id.equals(preferences.getString("ssh_id", ""))) edit.remove("ssh_id").putBoolean("paired", false);
        edit.remove(LAST_CONNECTED_PREFIX + ConnectionFailure.computerKey(ComputerSwitch.SSH_PREFIX + id)).apply();
    }

    /**
     * This phone's own SSH key: an ed25519 seed, made on first use and kept encrypted
     * like the pairing key. Every SSH Computer is given the same public key.
     */
    byte[] sshKey() {
        String stored = preferences.getString("ssh_key", "");
        if (!stored.isEmpty()) return Base64.decode(decrypt(stored), Base64.NO_WRAP);
        byte[] seed = new byte[32];
        new java.security.SecureRandom().nextBytes(seed);
        preferences.edit().putString("ssh_key", encrypt(Base64.encodeToString(seed, Base64.NO_WRAP))).apply();
        return seed;
    }

    /** The computer selected for remote work, or null. */
    PairedServer pairedServer() {
        String id = preferences.getString("server_id", "");
        if (!PairedServers.validId(id)) return null;
        return new PairedServer(id, preferences.getString("server_name", ""));
    }

    /** Every computer this phone is paired with, in pairing order. */
    java.util.List<PairedServer> pairedServers() {
        java.util.List<PairedServer> servers = PairedServers.parse(preferences.getString("servers", ""));
        // Before several computers were kept, only the selected one was stored.
        PairedServer selected = pairedServer();
        if (selected != null && PairedServers.find(servers, selected.id) == null) servers = PairedServers.with(servers, selected);
        return servers;
    }

    /** Pairing adds the computer, or renames it, and selects it. */
    void savePaired(PairedServer server) {
        preferences.edit().remove("ssh_id").putString("servers", PairedServers.format(PairedServers.with(pairedServers(), server)))
            .putString("server_id", server.id).putString("server_name", server.name)
            .putBoolean("paired", true).putBoolean("configured", true).apply();
    }

    /**
     * Stops knowing a computer. Its trust in this phone ends when a client there removes it.
     * Forgetting the selected one leaves this phone as the computer in use.
     */
    void forgetPaired(String id) {
        SharedPreferences.Editor edit = preferences.edit().putString("servers", PairedServers.format(PairedServers.without(pairedServers(), id)));
        PairedServer selected = pairedServer();
        if (selected != null && selected.id.equals(id)) edit.remove("server_id").remove("server_name").putBoolean("paired", false);
        edit.remove(RELAY_PREFIX + id).remove(LAST_CONNECTED_PREFIX + ConnectionFailure.computerKey(id)).apply();
    }

    /**
     * The computer in use as a {@link ComputerSwitch} target: a paired computer's id, an SSH
     * Computer's prefixed id, or null for this phone.
     */
    String inUseTarget() {
        if (usesPaired()) return pairedServer().id;
        SshComputers.Computer ssh = usesSsh() ? sshComputer() : null;
        return ssh != null ? ComputerSwitch.SSH_PREFIX + ssh.id : null;
    }

    /** Files the time the workspace of [target] (null for this phone) last opened. */
    void recordConnected(String target, long timeMillis) {
        preferences.edit().putLong(LAST_CONNECTED_PREFIX + ConnectionFailure.computerKey(target), timeMillis).apply();
    }

    /** When the workspace of [target] last opened, or 0 when this phone has no record. */
    long lastConnected(String target) { return preferences.getLong(LAST_CONNECTED_PREFIX + ConnectionFailure.computerKey(target), 0); }

    /** The relay a computer was last reached through, or null. It only shortens the next connect. */
    String relay(String id) { return preferences.getString(RELAY_PREFIX + id, null); }

    void saveRelay(String id, String relay) { preferences.edit().putString(RELAY_PREFIX + id, relay).apply(); }

    /** Selects one of the paired computers; returns false for one this phone does not know. */
    boolean selectPaired(String id) {
        PairedServer server = PairedServers.find(pairedServers(), id);
        if (server == null) return false;
        preferences.edit().remove("ssh_id").putString("server_id", server.id).putString("server_name", server.name)
            .putBoolean("paired", true).putBoolean("configured", true).apply();
        return true;
    }

    /**
     * This phone's own workspace has started at least once, so the switcher may offer it.
     * Until then the phone is set up from Settings instead (ADR-0070).
     */
    boolean localWorkspaceStarted() { return preferences.getBoolean("local_started", false); }

    void markLocalWorkspaceStarted() {
        if (!localWorkspaceStarted()) preferences.edit().putBoolean("local_started", true).apply();
    }

    /** Keeps the selected computer remembered while work moves to this phone. */
    void selectLocal() { preferences.edit().remove("ssh_id").putBoolean("paired", false).putBoolean("configured", true).apply(); }

    /** This phone's private key as a Remote Device, or null before the first pairing. */
    byte[] deviceKey() {
        String stored = preferences.getString("device_key", "");
        return stored.isEmpty() ? null : Base64.decode(decrypt(stored), Base64.NO_WRAP);
    }

    void saveDeviceKey(byte[] key) {
        preferences.edit().putString("device_key", encrypt(Base64.encodeToString(key, Base64.NO_WRAP))).apply();
    }

    private SecretKey key() throws Exception {
        KeyStore store = KeyStore.getInstance("AndroidKeyStore");
        store.load(null);
        if (!store.containsAlias("openaide-connections")) {
            KeyGenerator generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore");
            generator.init(new KeyGenParameterSpec.Builder("openaide-connections",
                KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build());
            generator.generateKey();
        }
        return (SecretKey) store.getKey("openaide-connections", null);
    }

    private String encrypt(String value) {
        try {
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(Cipher.ENCRYPT_MODE, key());
            return Base64.encodeToString(cipher.getIV(), Base64.NO_WRAP) + ":"
                + Base64.encodeToString(cipher.doFinal(value.getBytes(StandardCharsets.UTF_8)), Base64.NO_WRAP);
        } catch (Exception error) { throw new IllegalStateException("Secure storage unavailable"); }
    }

    private String decrypt(String value) {
        try {
            String[] parts = value.split(":", 2);
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(Cipher.DECRYPT_MODE, key(), new GCMParameterSpec(128, Base64.decode(parts[0], Base64.NO_WRAP)));
            return new String(cipher.doFinal(Base64.decode(parts[1], Base64.NO_WRAP)), StandardCharsets.UTF_8);
        } catch (Exception error) { throw new IllegalStateException("Secure storage unavailable"); }
    }
}
