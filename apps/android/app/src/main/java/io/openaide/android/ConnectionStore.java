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
    ConnectionStore(Context context) { preferences = context.getSharedPreferences("connection", Context.MODE_PRIVATE); }

    ConnectionProfile load() {
        if (preferences.getBoolean("remote", false)) {
            return new ConnectionProfile(preferences.getString("remote_url", ""),
                preferences.getString("remote_user", ""), decrypt(preferences.getString("remote_secret", "")), false);
        }
        return local();
    }

    ConnectionProfile local() { return new ConnectionProfile("http://127.0.0.1:5474/", "android", preferences.getString("password", ""), true); }

    void saveRemote(ConnectionProfile profile) {
        String secret = encrypt(profile.password);
        preferences.edit().putString("remote_url", profile.endpoint).putString("remote_user", profile.username)
            .putString("remote_secret", secret).putBoolean("remote", true).putBoolean("configured", true).apply();
    }

    void selectLocal() { preferences.edit().putBoolean("remote", false).putBoolean("configured", true).apply(); }

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
        } catch (Exception error) { throw new IllegalStateException("Secure credential storage unavailable"); }
    }

    private String decrypt(String value) {
        try {
            String[] parts = value.split(":", 2);
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(Cipher.DECRYPT_MODE, key(), new GCMParameterSpec(128, Base64.decode(parts[0], Base64.NO_WRAP)));
            return new String(cipher.doFinal(Base64.decode(parts[1], Base64.NO_WRAP)), StandardCharsets.UTF_8);
        } catch (Exception error) { throw new IllegalStateException("Please enter the server credentials again"); }
    }
}
