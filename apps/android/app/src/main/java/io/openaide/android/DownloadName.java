package io.openaide.android;

import java.io.ByteArrayOutputStream;
import java.nio.ByteBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * The name a workspace download is saved under. The App Server names the file in
 * Content-Disposition; the name is reduced to one plain file name because it comes
 * from the workspace, not from the user.
 */
final class DownloadName {
    private static final int MAX_LENGTH = 120;
    private static final Pattern EXTENDED = Pattern.compile("(?i)filename\\*\\s*=\\s*UTF-8''([^;\\s]+)");
    private static final Pattern QUOTED = Pattern.compile("(?i)(?:^|;)\\s*filename\\s*=\\s*\"((?:[^\"\\\\]|\\\\.)*)\"");
    private static final Pattern TOKEN = Pattern.compile("(?i)(?:^|;)\\s*filename\\s*=\\s*([^;\"\\s]+)");

    private DownloadName() { }

    static String from(String disposition, String url) {
        if (disposition != null) {
            Matcher extended = EXTENDED.matcher(disposition);
            String name = extended.find() ? clean(percentDecoded(extended.group(1))) : null;
            if (name != null) return name;
            Matcher quoted = QUOTED.matcher(disposition);
            name = quoted.find() ? clean(quoted.group(1).replaceAll("\\\\(.)", "$1")) : null;
            if (name != null) return name;
            Matcher token = TOKEN.matcher(disposition);
            name = token.find() ? clean(token.group(1)) : null;
            if (name != null) return name;
        }
        String path = url == null ? "" : url.replaceFirst("[?#].*$", "").replaceFirst("^[a-z]+://[^/]*", "");
        String name = clean(percentDecoded(path));
        return name != null ? name : "download";
    }

    /** The last path segment without control or reserved characters, or null when nothing usable is left. */
    private static String clean(String value) {
        if (value == null) return null;
        String name = value.substring(Math.max(value.lastIndexOf('/'), value.lastIndexOf('\\')) + 1)
            .replaceAll("[\\p{Cntrl}<>:\"|?*]", "_")
            .replaceAll("^[.\\s]+|[.\\s]+$", "");
        if (name.isEmpty()) return null;
        if (name.length() <= MAX_LENGTH) return name;
        int dot = name.lastIndexOf('.');
        String extension = dot > 0 && name.length() - dot <= 16 ? name.substring(dot) : "";
        return name.substring(0, MAX_LENGTH - extension.length()) + extension;
    }

    /** RFC 3986 decoding as UTF-8; '+' stays literal. Null for malformed input. */
    private static String percentDecoded(String value) {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        int index = 0;
        while (index < value.length()) {
            int escape = value.indexOf('%', index);
            int end = escape < 0 ? value.length() : escape;
            byte[] literal = value.substring(index, end).getBytes(StandardCharsets.UTF_8);
            bytes.write(literal, 0, literal.length);
            if (escape < 0) break;
            if (escape + 2 >= value.length()) return null;
            int high = Character.digit(value.charAt(escape + 1), 16);
            int low = Character.digit(value.charAt(escape + 2), 16);
            if (high < 0 || low < 0) return null;
            bytes.write(high * 16 + low);
            index = escape + 3;
        }
        try {
            return StandardCharsets.UTF_8.newDecoder()
                .onMalformedInput(CodingErrorAction.REPORT).onUnmappableCharacter(CodingErrorAction.REPORT)
                .decode(ByteBuffer.wrap(bytes.toByteArray())).toString();
        } catch (CharacterCodingException error) { return null; }
    }
}
