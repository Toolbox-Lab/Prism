import * as vscode from "vscode";

export function registerCodeLens(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.languages.registerCodeLensProvider("rust", {
      provideCodeLenses(document) {
        return [];
      },
    }),
  );
}
