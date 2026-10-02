import * as vscode from "vscode";

export function registerQuickFix(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.languages.registerCodeActionsProvider("rust", {
      provideCodeActions(document, range, context) {
        return [];
      },
    }),
  );
}
