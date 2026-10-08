import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.regex.*;

/**
 * Stdio proxy between Zed and the SonarLint language server.
 *
 * SonarLint sends custom "sonarlint/*" requests that only VS Code's client implements. Zed ignores
 * them, so the server never publishes diagnostics. This proxy answers those requests itself and
 * forwards everything else untouched in both directions.
 *
 * Usage: java LspProxy.java <args for the real java process...>
 */
public class LspProxy {
    static final Pattern METHOD = Pattern.compile("\"method\"\\s*:\\s*\"(sonarlint/[A-Za-z]+)\"");
    static final Pattern ID = Pattern.compile("\"id\"\\s*:\\s*(\\d+|\"[^\"]*\")");
    static final Pattern FILE_URIS = Pattern.compile("\"fileUris\"\\s*:\\s*(\\[[^\\]]*\\])");
    static final Object SERVER_LOCK = new Object();
    static final Object CLIENT_LOCK = new Object();

    public static void main(String[] args) throws Exception {
        List<String> cmd = new ArrayList<>();
        cmd.add(System.getProperty("java.home") + File.separator + "bin" + File.separator + "java");
        cmd.addAll(Arrays.asList(args));
        Process server = new ProcessBuilder(cmd).redirectError(ProcessBuilder.Redirect.INHERIT).start();
        OutputStream toServer = server.getOutputStream();
        OutputStream toClient = new BufferedOutputStream(System.out);

        Thread clientToServer = new Thread(() -> {
            try {
                InputStream in = new BufferedInputStream(System.in);
                byte[] msg;
                while ((msg = readMessage(in)) != null) {
                    write(toServer, SERVER_LOCK, msg);
                }
            } catch (IOException ignored) {
            } finally {
                server.destroy();
            }
        });
        clientToServer.setDaemon(true);
        clientToServer.start();

        InputStream in = new BufferedInputStream(server.getInputStream());
        byte[] msg;
        while ((msg = readMessage(in)) != null) {
            String body = body(msg);
            byte[] reply = answer(body);
            if (reply != null) {
                write(toServer, SERVER_LOCK, frame(reply));
            } else {
                write(toClient, CLIENT_LOCK, msg);
            }
        }
        System.exit(server.waitFor());
    }

    /** Returns a JSON-RPC response for server->client requests Zed cannot handle, else null. */
    static byte[] answer(String body) {
        Matcher m = METHOD.matcher(body);
        Matcher id = ID.matcher(body);
        if (!m.find() || !id.find()) return null; // notification or not a request
        String result;
        switch (m.group(1)) {
            case "sonarlint/isOpenInEditor": result = "true"; break;
            case "sonarlint/shouldAnalyseFile": result = "{\"shouldBeAnalysed\":true}"; break;
            case "sonarlint/filterOutExcludedFiles": {
                Matcher f = FILE_URIS.matcher(body);
                result = "{\"fileUris\":" + (f.find() ? f.group(1) : "[]") + "}";
                break;
            }
            case "sonarlint/listFilesInFolder": result = "{\"foundFiles\":[]}"; break;
            case "sonarlint/isIgnoredByScm": result = "false"; break;
            case "sonarlint/hasJoinedIdeLabs": result = "false"; break;
            case "sonarlint/getJavaConfig": result = "null"; break;
            case "sonarlint/getTokenForServer": result = "null"; break;
            default: return null;
        }
        return ("{\"jsonrpc\":\"2.0\",\"id\":" + id.group(1) + ",\"result\":" + result + "}")
            .getBytes(StandardCharsets.UTF_8);
    }

    static byte[] readMessage(InputStream in) throws IOException {
        ByteArrayOutputStream header = new ByteArrayOutputStream();
        int len = -1;
        while (true) {
            ByteArrayOutputStream line = new ByteArrayOutputStream();
            int c;
            while ((c = in.read()) != -1 && c != '\n') line.write(c);
            if (c == -1) return null;
            header.write(line.toByteArray());
            header.write('\n');
            String s = line.toString(StandardCharsets.US_ASCII).trim();
            if (s.isEmpty()) break;
            if (s.toLowerCase().startsWith("content-length:")) len = Integer.parseInt(s.substring(15).trim());
        }
        if (len < 0) return null;
        byte[] bodyBytes = in.readNBytes(len);
        if (bodyBytes.length < len) return null;
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        out.write(header.toByteArray());
        out.write(bodyBytes);
        return out.toByteArray();
    }

    static String body(byte[] msg) {
        for (int i = 0; i + 3 < msg.length; i++) {
            if (msg[i] == '\r' && msg[i + 1] == '\n' && msg[i + 2] == '\r' && msg[i + 3] == '\n') {
                return new String(msg, i + 4, msg.length - i - 4, StandardCharsets.UTF_8);
            }
        }
        return "";
    }

    static byte[] frame(byte[] body) {
        byte[] h = ("Content-Length: " + body.length + "\r\n\r\n").getBytes(StandardCharsets.US_ASCII);
        byte[] out = Arrays.copyOf(h, h.length + body.length);
        System.arraycopy(body, 0, out, h.length, body.length);
        return out;
    }

    static void write(OutputStream out, Object lock, byte[] msg) throws IOException {
        synchronized (lock) {
            out.write(msg);
            out.flush();
        }
    }
}
