// Nudge VS Code extension — language support, diagnostics via `nudgec lsp`,
// document formatting via `nudgec fmt`, commands, and trace debugging over DAP.
const vscode = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");
const { execFile } = require("child_process");
const fs = require("fs");
const os = require("os");
const path = require("path");

/** @type {LanguageClient | undefined} */
let client;

/** @type {vscode.OutputChannel | undefined} */
let outputChannel;

function getOutputChannel() {
  if (!outputChannel) {
    outputChannel = vscode.window.createOutputChannel("Nudge");
  }
  return outputChannel;
}

function runNudgeCommand(subcmd, extraArgs = []) {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    vscode.window.showWarningMessage(`Open a Nudge (.ndg) file to run 'nudgec ${subcmd}'`);
    return;
  }
  const config = vscode.workspace.getConfiguration("nudge");
  const serverPath = config.get("serverPath", "nudgec");
  const filePath = editor.document.fileName;
  const channel = getOutputChannel();

  channel.show(true);
  channel.appendLine(`\n--- [nudgec ${subcmd}] ${filePath} ---`);

  execFile(serverPath, [subcmd, ...extraArgs, filePath], { cwd: path.dirname(filePath) }, (err, stdout, stderr) => {
    if (stdout) channel.append(stdout);
    if (stderr) channel.append(stderr);
    if (err) {
      channel.appendLine(`[nudgec ${subcmd}] exited with error code ${err.code || 1}`);
    } else {
      channel.appendLine(`[nudgec ${subcmd}] finished successfully.`);
    }
  });
}

function activate(context) {
  const config = vscode.workspace.getConfiguration("nudge");
  const serverPath = config.get("serverPath", "nudgec");

  // 1. Language Server Protocol (diagnostics, hover, definition, completion)
  const serverOptions = {
    command: serverPath,
    args: ["lsp"],
    transport: TransportKind.stdio,
  };

  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "nudge" }],
    trace: config.get("trace.server", "off"),
  };

  client = new LanguageClient(
    "nudge",
    "Nudge Language Server",
    serverOptions,
    clientOptions
  );

  context.subscriptions.push(client.start());

  // 2. Document formatting provider via `nudgec fmt`
  const formattingProvider = vscode.languages.registerDocumentFormattingEditProvider("nudge", {
    provideDocumentFormattingEdits(document) {
      return new Promise((resolve) => {
        const text = document.getText();
        const tmpFile = path.join(
          os.tmpdir(),
          `nudge_fmt_${Date.now()}_${Math.random().toString(36).substring(2)}.ndg`
        );

        fs.writeFile(tmpFile, text, "utf8", (err) => {
          if (err) return resolve([]);

          const currentServerPath = vscode.workspace.getConfiguration("nudge").get("serverPath", "nudgec");
          execFile(currentServerPath, ["fmt", tmpFile], (err2) => {
            if (err2) {
              fs.unlink(tmpFile, () => {});
              return resolve([]);
            }
            fs.readFile(tmpFile, "utf8", (err3, formatted) => {
              fs.unlink(tmpFile, () => {});
              if (err3 || !formatted || formatted === text) {
                return resolve([]);
              }
              const fullRange = new vscode.Range(
                document.positionAt(0),
                document.positionAt(text.length)
              );
              resolve([vscode.TextEdit.replace(fullRange, formatted)]);
            });
          });
        });
      });
    },
  });
  context.subscriptions.push(formattingProvider);

  // 3. Debug Adapter Protocol factory for `nudgec debug <trace.jsonl>`
  const debugAdapterFactory = vscode.debug.registerDebugAdapterDescriptorFactory("nudge", {
    createDebugAdapterDescriptor(session) {
      const currentServerPath = vscode.workspace.getConfiguration("nudge").get("serverPath", "nudgec");
      const tracePath = session.configuration.trace;
      if (!tracePath) {
        vscode.window.showErrorMessage("Trace path must be specified in Nudge debug launch configuration");
        return undefined;
      }
      return new vscode.DebugAdapterExecutable(currentServerPath, ["debug", tracePath]);
    },
  });
  context.subscriptions.push(debugAdapterFactory);

  // 4. Editor commands
  context.subscriptions.push(
    vscode.commands.registerCommand("nudge.check", () => runNudgeCommand("check")),
    vscode.commands.registerCommand("nudge.build", () => runNudgeCommand("build")),
    vscode.commands.registerCommand("nudge.buildTs", () => runNudgeCommand("build-ts")),
    vscode.commands.registerCommand("nudge.test", () => runNudgeCommand("test")),
    vscode.commands.registerCommand("nudge.cost", () => runNudgeCommand("cost")),
    vscode.commands.registerCommand("nudge.formatDocument", () => vscode.commands.executeCommand("editor.action.formatDocument")),
    vscode.commands.registerCommand("nudge.traceView", async () => {
      const activeFile = vscode.window.activeTextEditor?.document.fileName;
      let traceFile = activeFile?.endsWith(".jsonl") ? activeFile : undefined;
      if (!traceFile) {
        const uris = await vscode.window.showOpenDialog({
          canSelectMany: false,
          filters: { "Trace Files": ["jsonl"] },
          openLabel: "Open Trace",
        });
        if (uris && uris.length > 0) {
          traceFile = uris[0].fsPath;
        }
      }
      if (traceFile) {
        const currentServerPath = vscode.workspace.getConfiguration("nudge").get("serverPath", "nudgec");
        const channel = getOutputChannel();
        channel.show(true);
        channel.appendLine(`[Nudge] Launching trace view for ${traceFile}...`);
        execFile(currentServerPath, ["trace-view", traceFile], (err, stdout, stderr) => {
          if (stdout) channel.append(stdout);
          if (stderr) channel.append(stderr);
          if (err) channel.appendLine(`[Nudge] trace-view exited: ${err.message}`);
        });
      }
    }),
    vscode.commands.registerCommand("nudge.restartServer", async () => {
      if (client) {
        const channel = getOutputChannel();
        channel.appendLine("[Nudge] Restarting language server...");
        await client.stop();
        client.start();
        channel.appendLine("[Nudge] Language server restarted.");
        vscode.window.showInformationMessage("Nudge Language Server restarted.");
      }
    })
  );
}

function deactivate() {
  if (!client) return undefined;
  return client.stop();
}

module.exports = { activate, deactivate };
