use std::fs;
use zed_extension_api::{
    self as zed, serde_json::{json, Value}, settings::LspSettings, LanguageServerId, Result,
};

// The SonarLint VS Code release bundles the language server, analyzers and eslint bridge.
const VSIX_VERSION: &str = "6.0.1";
const RELEASE_TAG: &str = "6.0.1%2B91054";
const ANALYZERS: &[&str] = &[
    "sonargo.jar",
    "sonarjava.jar",
    "sonarjavasymbolicexecution.jar",
    "sonarjs.jar",
    "sonarphp.jar",
    "sonarpython.jar",
    "sonarhtml.jar",
    "sonarxml.jar",
    "sonartext.jar",
    "sonariac.jar",
];

const PROXY_SOURCE: &str = include_str!("../proxy/LspProxy.java");

/// Writes the proxy source next to the downloaded server and returns its absolute path.
fn write_proxy() -> Result<String> {
    let dir = std::env::current_dir().map_err(|e| e.to_string())?.join("sonarlint-proxy");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join("LspProxy.java");
    fs::write(&file, PROXY_SOURCE).map_err(|e| e.to_string())?;
    Ok(file.to_string_lossy().to_string())
}

struct SonarLintExtension;

fn user_settings(worktree: &zed::Worktree) -> Value {
    LspSettings::for_worktree("sonarlint", worktree)
        .ok()
        .and_then(|s| s.settings)
        .unwrap_or(Value::Null)
}

fn setting_str(settings: &Value, pointer: &str) -> Option<String> {
    settings
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(String::from)
}

/// Node executable: `sonarlint.pathToNodeExecutable` setting, else `node` on PATH.
fn node_path(worktree: &zed::Worktree) -> Option<String> {
    setting_str(&user_settings(worktree), "/sonarlint/pathToNodeExecutable")
        .or_else(|| worktree.which("node"))
}

impl SonarLintExtension {
    /// Returns the absolute path of the unpacked `extension/` directory from the VSIX.
    fn ensure_installed(&self, id: &LanguageServerId) -> Result<String> {
        let dir = format!("sonarlint-{VSIX_VERSION}");
        let root = format!("{dir}/extension");
        if !fs::metadata(format!("{root}/server/sonarlint-ls.jar")).is_ok() {
            zed::set_language_server_installation_status(
                id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );
            let url = format!(
                "https://github.com/SonarSource/sonarlint-vscode/releases/download/{RELEASE_TAG}/sonarlint-vscode-{VSIX_VERSION}.vsix"
            );
            zed::download_file(&url, &dir, zed::DownloadedFileType::Zip)
                .map_err(|e| format!("failed to download SonarLint: {e}"))?;
            if let Ok(entries) = fs::read_dir(".") {
                for e in entries.flatten() {
                    let n = e.file_name().to_string_lossy().to_string();
                    if n.starts_with("sonarlint-") && n != dir {
                        fs::remove_dir_all(e.path()).ok();
                    }
                }
            }
        }
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        Ok(cwd.join(root).to_string_lossy().to_string())
    }
}

impl zed::Extension for SonarLintExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let root = self.ensure_installed(id)?;
        let settings = user_settings(worktree);
        let java_override = setting_str(&settings, "/sonarlint/ls/javaHome")
            .map(|h| format!("{}/bin/java", h.trim_end_matches('/')));
        // The server needs Java 21+. On macOS let java_home choose a suitable JDK.
        let (os, _) = zed::current_platform();
        let (command, mut args) = match (java_override, os) {
            (Some(j), _) => (j, vec![]),
            (None, zed::Os::Mac) => (
                "/usr/libexec/java_home".to_string(),
                vec!["-v".into(), "21+".into(), "--exec".into(), "java".into()],
            ),
            (None, _) => (
                worktree.which("java").ok_or(
                    "SonarLint needs Java 21+ on PATH (or set lsp.sonarlint.settings.sonarlint.ls.javaHome)",
                )?,
                vec![],
            ),
        };
        // The proxy (Java source-launch) answers SonarLint's VS Code-only requests, then runs the
        // real server with the remaining arguments.
        let proxy = write_proxy()?;
        args.push(proxy);
        args.extend([
            "-Dsonarlint.telemetry.disabled=true".to_string(),
        ]);
        if let Some(vm) = setting_str(&settings, "/sonarlint/ls/vmargs") {
            args.extend(vm.split_whitespace().map(String::from));
        }
        args.extend([
            "-jar".to_string(),
            format!("{root}/server/sonarlint-ls.jar"),
            "-stdio".into(),
            "-analyzers".into(),
        ]);
        args.extend(ANALYZERS.iter().map(|a| format!("{root}/analyzers/{a}")));
        Ok(zed::Command { command, args, env: worktree.shell_env() })
    }

    fn language_server_initialization_options(
        &mut self,
        id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<Value>> {
        let root = self.ensure_installed(id)?;
        let user = LspSettings::for_worktree("sonarlint", worktree)
            .ok()
            .and_then(|s| s.initialization_options);
        let node = node_path(worktree);
        let (os, cpu) = zed::current_platform();
        let platform = match os {
            zed::Os::Mac => "darwin",
            zed::Os::Linux => "linux",
            zed::Os::Windows => "win32",
        };
        let arch = match cpu {
            zed::Architecture::Aarch64 => "arm64",
            zed::Architecture::X86 => "ia32",
            zed::Architecture::X8664 => "x64",
        };
        let mut opts = json!({
            "productKey": "zed",
            "productName": "SonarLint Zed",
            "productVersion": "0.1.0",
            "workspaceName": "zed",
            "firstSecretDetected": true,
            "showVerboseLogs": false,
            "platform": platform,
            "architecture": arch,
            "additionalAttributes": {},
            "enableNotebooks": false,
            "clientNodePath": node,
            "eslintBridgeServerPath": format!("{root}/eslint-bridge"),
            "connections": { "sonarqube": [], "sonarcloud": [] },
            "rules": {},
            "focusOnNewCode": false,
            "automaticAnalysis": true,
            "telemetryStorage": format!("{root}/../usage"),
        });
        if let (Some(Value::Object(u)), Some(o)) = (user, opts.as_object_mut()) {
            o.extend(u);
        }
        Ok(Some(opts))
    }

    fn language_server_workspace_configuration(
        &mut self,
        _id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<Value>> {
        let user = LspSettings::for_worktree("sonarlint", worktree)
            .ok()
            .and_then(|s| s.settings)
            .unwrap_or(Value::Null);
        let mut cfg = json!({ "sonarlint": {
            "rules": {},
            "automaticAnalysis": true,
            "focusOnNewCode": false,
            "pathToNodeExecutable": node_path(worktree),
            "disableTelemetry": true,
            "testFilePattern": "",
            "analysisExcludesStandalone": "",
            "analyzerProperties": {},
            "output": { "showVerboseLogs": false },
        }});
        merge(&mut cfg, user);
        Ok(Some(cfg))
    }
}

fn merge(base: &mut Value, over: Value) {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                merge(b.entry(k).or_insert(Value::Null), v);
            }
        }
        (b, o) => *b = o,
    }
}

zed::register_extension!(SonarLintExtension);
