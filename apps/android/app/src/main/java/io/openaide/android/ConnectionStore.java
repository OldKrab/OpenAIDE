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
    }

    ConnectionProfile load() { return usesPaired() ? ConnectionProfile.paired() : local(); }

    /** Whether the paired computer, rather than this phone, is the selected workspace. */
    boolean usesPaired() { return preferences.getBoolean("paired", false) && pairedServer() != null; }

    ConnectionProfile local() { return new ConnectionProfile(ConnectionProfile.LOCAL_ENDPOINT, "android", preferences.getString("password", ""), true); }

    PairedServer pairedServer() {
        String id = preferences.getString("server_id", "");
        if (!id.matches("[0-9a-f]{64}")) return null;
        return new PairedServer(id, preferences.getString("server_name", ""));
    }

    /** Pairing selects the computer; the previous one, if any, is replaced. */
    void savePaired(PairedServer server) {
        preferences.edit().putString("server_id", server.id).putString("server_name", server.name)
            .putBoolean("paired", true).putBoolean("configured", true).apply();
    }

    /** Stops using the paired computer. Its trust in this phone ends when a client there removes it. */
    void forgetPaired() { preferences.edit().remove("server_id").remove("server_name").putBoolean("paired", false).apply(); }

    /** Returns to the paired computer after working on this phone; trust is unchanged. */
    void selectPaired() { if (pairedServer() != null) preferences.edit().putBoolean("paired", true).putBoolean("configured", true).apply(); }

    void selectLocal() { preferences.edit().putBoolean("paired", false).putBoolean("configured", true).apply(); }

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
