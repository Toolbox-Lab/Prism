import * as vscode from "vscode";
import { registerDiagnostics } from "./providers/diagnostics";
import { registerHoverProvider } from "./providers/hover";
import { registerCodeLens } from "./providers/codeLens";
import { registerQuickFix } from "./providers/quickFix";
import { callGrat } from "./cli/bridge";

export function activate(context: vscode.ExtensionContext) {
  console.log("Grat extension activated");

  registerDiagnostics(context);
  registerHoverProvider(context);
  registerCodeLens(context);
  registerQuickFix(context);

  // In extension.ts, replace the stub registration:

  context.subscriptions.push(
    vscode.commands.registerCommand("grat.decode", async () => {
      // 1. Capture input from the user
      const input = await vscode.window.showInputBox({
        prompt: "Paste a raw XDR string or transaction hash",
        placeHolder: "AAAAAgAAAAA... or a tx hash",
        ignoreFocusOut: true, // don't dismiss if focus shifts
        validateInput: (value) => {
          return value.trim().length === 0 ? "Input cannot be empty" : null;
        },
      });

      // User hit Escape or left it empty — bail out quietly
      if (!input) {
        return;
      }

      // 2. Call the CLI via the bridge, with progress feedback
      try {
        const result = await vscode.window.withProgress(
          {
            location: vscode.ProgressLocation.Notification,
            title: "Decoding transaction...",
            cancellable: false,
          },
          async () => {
            return await callGrat(["decode", input.trim()]);
          },
        );

        // 3. Format and display in a new read-only editor
        const formatted = JSON.stringify(result, null, 2);

        const doc = await vscode.workspace.openTextDocument({
          content: formatted,
          language: "json",
        });

        const editor = await vscode.window.showTextDocument(doc, {
          preview: false,
          viewColumn: vscode.ViewColumn.Beside,
        });

        // Make it read-only so users don't accidentally "edit" decoded output
        await vscode.languages.setTextDocumentLanguage(doc, "json");
        // Enforce read-only via editor options (VSCode has no native "readonly doc"
        // for untitled docs, so this is usually done via a FileSystemProvider —
        // see note below)
      } catch (err: any) {
        vscode.window.showErrorMessage(
          `Grat decode failed: ${err.message ?? err}`,
        );
      }
    }),
  );
}

export function deactivate() {}
