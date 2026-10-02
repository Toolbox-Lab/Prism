import * as vscode from "vscode";

export function registerDiagnostics(context: vscode.ExtensionContext) {
  const diagnostics = vscode.languages.createDiagnosticCollection("grat");
  context.subscriptions.push(diagnostics);
}
