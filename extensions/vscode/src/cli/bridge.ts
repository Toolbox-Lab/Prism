import { execFile } from "child_process";

export function callGrat(args: string[]): Promise<unknown> {
  return new Promise((resolve, reject) => {
    execFile("grat", [...args, "--output", "json"], (error, stdout, stderr) => {
      if (error) {
        const stderrMsg = stderr ? stderr.trim() : "";
        const message = stderrMsg
          ? "grat CLI error: " + stderrMsg
          : "grat CLI failed: " + (error.message || "Unknown error");
        return reject(new Error(message));
      }
      try {
        resolve(JSON.parse(stdout));
      } catch {
        const stderrMsg = stderr ? stderr.trim() : "";
        const message = stderrMsg
          ? "Failed to parse grat output: " + stderrMsg
          : "Failed to parse grat output";
        reject(new Error(message));
      }
    });
  });
}
