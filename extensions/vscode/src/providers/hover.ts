import * as vscode from "vscode";

export function registerHoverProvider(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.languages.registerHoverProvider("rust", {
      provideHover(document, position) {
        return null;
      },
    }),
  );
}
