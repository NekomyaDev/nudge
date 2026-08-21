// Nudge VS Code extension — wires `nudgec lsp` (stdio) for .ndg diagnostics.
const vscode = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");

/** @type {LanguageClient | undefined} */
let client;

function activate(context) {
  const config = vscode.workspace.getConfiguration("nudge");
  const serverPath = config.get("serverPath", "nudgec");

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
}

function deactivate() {
  if (!client) return undefined;
  return client.stop();
}

module.exports = { activate, deactivate };
