# SonarLint for Zed

Bring SonarLint (SonarQube for IDE) to the [Zed](https://zed.dev) editor. Bugs, vulnerabilities,
code smells and hard-coded secrets show up as inline diagnostics while you type, using the same
analysis engine as the official SonarLint VS Code extension.

This extension is not affiliated with or endorsed by SonarSource. It downloads the SonarLint
language server and analyzers from SonarSource's official VS Code release (LGPL-3.0, see
[SonarSource/sonarlint-vscode](https://github.com/SonarSource/sonarlint-vscode)).

## Supported languages

JavaScript, TypeScript, TSX, JSX, Python, Java, PHP, HTML, CSS, Go, YAML, JSON, XML, Dockerfile, Terraform, Rust.

**Rust:** the SonarLint bundle has no Rust code analyzer, so Rust files get **secrets detection only**
(for example leaked GitHub, Slack or cloud tokens). Bugs and code smells for Rust come from
`rust-analyzer` and `clippy`, not from SonarLint. Rust is enabled for secrets because those checks
apply to every file type.

## How it works

```
 Zed editor
    |  LSP over stdio (diagnostics, document sync, settings)
    v
 +---------------------------+   written to the extension work dir at startup
 | Extension (Rust -> WASM)  |   - downloads and unpacks the SonarLint bundle once
 |  src/lib.rs               |   - writes proxy/LspProxy.java
 +---------------------------+   - builds the launch command, init options and settings
    |  spawns:  java LspProxy.java -jar sonarlint-ls.jar -stdio -analyzers ...
    v
 +---------------------------+
 | LspProxy (Java, 1 file)   |   answers SonarLint's VS Code-only requests itself:
 |  proxy/LspProxy.java      |   isOpenInEditor, shouldAnalyseFile, listFilesInFolder, ...
 +---------------------------+   passes every other message through unchanged
    |  stdio
    v
 +---------------------------+     +-------------------------------------------+
 | SonarLint language server |---->| Analyzers (jars): JS/TS, Python, Java,    |
 |  sonarlint-ls.jar         |     | PHP, Go, HTML, XML, IaC, secrets/text      |
 +---------------------------+     +-------------------------------------------+
    |  JavaScript/TypeScript analysis only
    v
 Node.js (eslint-bridge)
```

Components:

| Part | Role |
|------|------|
| `extension.toml` | Declares the `sonarlint` language server and the languages it can attach to. |
| `src/lib.rs` | The Zed extension. Downloads the SonarLint VS Code release (`.vsix`, a zip) on first use, resolves Java and Node from your settings or the system, and starts the proxy. |
| `proxy/LspProxy.java` | A small stdio proxy, launched with Java's single-file source mode (no build step). |
| SonarLint bundle | `sonarlint-ls.jar` (server), `analyzers/*.jar` (rules) and `eslint-bridge` (JS/TS), downloaded from SonarSource's release. |

### Why a proxy is needed

The SonarLint server was written for VS Code. Besides standard LSP, it sends its own requests such as
`sonarlint/isOpenInEditor` and `sonarlint/shouldAnalyseFile` and waits for answers. Zed does not
implement them, so without help the server never decides a file should be analyzed and never publishes
diagnostics. The proxy answers those requests (a file is open, analyze it, no extra files, not
git-ignored) and forwards everything else untouched, so Zed shows normal inline diagnostics.

## Requirements

| Tool | Version | Needed for | Default lookup |
|------|---------|-----------|----------------|
| Java (JDK/JRE) | **21 or newer** | Running the language server and the proxy (always) | macOS: auto-detected via `/usr/libexec/java_home -v 21+`. Linux/Windows: `java` on `PATH` |
| Node.js | Recent LTS | JavaScript, TypeScript, JSX/TSX, CSS, HTML analysis | `node` on `PATH` |
| Rust (via rustup) | stable | Building the extension (dev install only) | n/a |

Java 17 or older will not work: the server fails with `UnsupportedClassVersionError`.

Check what you have:

```bash
java -version
node --version
```

## Installation

### From source (dev extension)

1. Install Rust with [rustup](https://rustup.rs) and add the WASM target:

   ```bash
   rustup target add wasm32-wasip1
   ```

2. Clone this repository.
3. In Zed, open the command palette and run **`zed: install dev extension`**, then select the
   cloned folder.
4. Enable the server for your languages (next section).

On first use the extension downloads the SonarLint bundle (about 170 MB, one time) into Zed's
extension data directory. The status appears in Zed's language server indicator.

## Enable SonarLint for your languages

Zed only starts a language server for languages that list it. Open `settings.json`
(**`zed: open settings`**) and add `sonarlint` to each language you want. Keep `"..."` so your
existing language servers keep working.

```json
{
  "languages": {
    "JavaScript": { "language_servers": ["sonarlint", "..."] },
    "TypeScript": { "language_servers": ["sonarlint", "..."] },
    "TSX":        { "language_servers": ["sonarlint", "..."] },
    "Python":     { "language_servers": ["sonarlint", "..."] },
    "Java":       { "language_servers": ["sonarlint", "..."] },
    "PHP":        { "language_servers": ["sonarlint", "..."] },
    "HTML":       { "language_servers": ["sonarlint", "..."] },
    "CSS":        { "language_servers": ["sonarlint", "..."] },
    "Go":         { "language_servers": ["sonarlint", "..."] },
    "Rust":       { "language_servers": ["sonarlint", "..."] },
    "YAML":       { "language_servers": ["sonarlint", "..."] },
    "Dockerfile": { "language_servers": ["sonarlint", "..."] }
  }
}
```

## Configuring Java and Node.js paths

If Java or Node.js are not on the `PATH` Zed sees (common when Zed is launched from the Dock or
Spotlight, or when you use nvm/SDKMAN/Homebrew keg-only installs), set explicit paths in
`settings.json`:

```json
{
  "lsp": {
    "sonarlint": {
      "settings": {
        "sonarlint": {
          "ls": {
            "javaHome": "/Library/Java/JavaVirtualMachines/temurin-21.jdk/Contents/Home",
            "vmargs": "-Xmx1g"
          },
          "pathToNodeExecutable": "/usr/local/bin/node"
        }
      }
    }
  }
}
```

| Setting (under `lsp.sonarlint.settings.sonarlint`) | Type | Description |
|---|---|---|
| `ls.javaHome` | string | Home directory of a JDK/JRE 21+. The extension runs `<javaHome>/bin/java`. Point it at the home folder, not at the `java` binary. |
| `pathToNodeExecutable` | string | Full path to the `node` executable, for example `/Users/you/.nvm/versions/node/v22.0.0/bin/node`. |
| `ls.vmargs` | string | Extra JVM arguments, space separated, for example `-Xmx1g`. |
| `rules` | object | Enable or disable rules, for example `{ "javascript:S1481": { "level": "off" } }`. |
| `analysisExcludesStandalone` | string | Comma-separated glob patterns to skip, for example `**/vendor/**,**/*.min.js`. |
| `testFilePattern` | string | Glob patterns that mark test files. |
| `analyzerProperties` | object | Extra analyzer properties passed through to SonarLint. |

Resolution order for each path:

1. The value you set in `settings.json`.
2. Java on macOS: `/usr/libexec/java_home -v 21+`. Java elsewhere and Node: the first match on `PATH`.

Finding your paths:

```bash
# macOS: list installed JDKs and their home directories
/usr/libexec/java_home -V

# Linux/macOS: where is node?
which node

# nvm users: path of the currently active node
nvm which current
```

On Windows use forward slashes or escaped backslashes, for example `"C:/Program Files/Java/jdk-21"`.

After changing any setting, restart the server: run **`editor: restart language server`** from the
command palette, or restart Zed.

## Troubleshooting

| Symptom | Likely cause and fix |
|---|---|
| No diagnostics at all | `sonarlint` is not listed under `languages.<Language>.language_servers`. Add it. |
| `SonarLint needs Java 21+ ...` | No Java found. Install JDK 21+ or set `ls.javaHome`. |
| `UnsupportedClassVersionError` in the log | Java is older than 21. Set `ls.javaHome` to a JDK 21+. |
| JS/TS files show no issues, other languages do | Node.js not found. Set `pathToNodeExecutable`. |
| Works in a terminal but not in Zed | Zed was launched without your shell environment. Set both paths explicitly. |
| Server runs but no SonarLint issues appear | The proxy is not in the launch command (old build). Click **Rebuild** on the dev extension and restart the language server. |
| Download fails | Check network and proxy access to `github.com`, then restart the server. |

Logs: open **`zed: open log`**, or launch Zed with `zed --foreground` to see the server's output.

## Limitations

This extension runs the standard SonarLint language server, which covers analysis and diagnostics.
Features that depend on custom VS Code UI are not available:

- Connected mode (SonarQube Server / SonarQube Cloud binding)
- Rule description side panel
- Quick fixes and "deactivate rule" code actions
- Security hotspot and taint vulnerability views

## Development

```bash
cargo build --target wasm32-wasip1 --release
```

Source layout:

- `extension.toml`: extension manifest and language mappings
- `src/lib.rs`: downloads the bundle, writes the proxy, builds the Java launch command, supplies initialization options and settings
- `proxy/LspProxy.java`: the stdio proxy; test changes by running the server through it with any LSP client

The pinned SonarLint version lives in `VSIX_VERSION` and `RELEASE_TAG` in `src/lib.rs`. Update both to
move to a newer release.

## License

Apache-2.0 for this extension's code (see [LICENSE](LICENSE)). The downloaded SonarLint components are licensed by SonarSource under their own terms.
