/**
 * OpenSpec Extension for pi
 *
 * Wraps the OpenSpec CLI as native pi tools and commands so the LLM can
 * drive spec-driven development without raw bash orchestration.
 *
 * Tools:
 *   openspec — multi-action tool (status, new, list, instructions, update)
 *
 * Commands:
 *   /opsx           — show dashboard (active changes, specs, status)
 *   /opsx:propose   — propose a new change (delegates to skill)
 *   /opsx:apply     — implement tasks from a change (delegates to skill)
 *   /opsx:explore   — explore ideas interactively (delegates to skill)
 *   /opsx:archive   — archive a completed change (delegates to skill)
 *
 * Footer:
 *   Shows active OpenSpec change name + progress in the status bar.
 */

import type { ExtensionAPI, ExtensionContext } from "@mariozechner/pi-coding-agent";
import { truncateHead, DEFAULT_MAX_LINES, DEFAULT_MAX_BYTES, formatSize } from "@mariozechner/pi-coding-agent";
import { StringEnum } from "@mariozechner/pi-ai";
import { Text } from "@mariozechner/pi-tui";
import { Type } from "@sinclair/typebox";
import { resolve } from "node:path";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Locate the openspec wrapper script (scripts/openspec) in the project. */
function openspecBin(cwd: string): string {
  return resolve(cwd, "scripts", "openspec");
}

/** Run the openspec CLI and return stdout. Throws on non-zero exit. */
async function runOpenspec(
  pi: ExtensionAPI,
  cwd: string,
  args: string[],
  signal?: AbortSignal,
): Promise<string> {
  const bin = openspecBin(cwd);
  const result = await pi.exec(bin, args, { signal, timeout: 30_000 });
  if (result.code !== 0) {
    const msg = (result.stderr || result.stdout || "").trim();
    throw new Error(`openspec ${args.join(" ")} failed (exit ${result.code}): ${msg}`);
  }
  return result.stdout;
}

/** Run openspec and return parsed JSON. */
async function runOpenspecJson<T = unknown>(
  pi: ExtensionAPI,
  cwd: string,
  args: string[],
  signal?: AbortSignal,
): Promise<T> {
  const out = await runOpenspec(pi, cwd, [...args, "--json"], signal);
  return JSON.parse(out) as T;
}

// ---------------------------------------------------------------------------
// Types for openspec CLI JSON output
// ---------------------------------------------------------------------------

interface ChangeInfo {
  name: string;
  schema?: string;
  status?: string;
  path?: string;
}

interface ArtifactInfo {
  id: string;
  status: string;
  dependencies?: string[];
  outputPath?: string;
}

interface StatusInfo {
  changeName?: string;
  schemaName?: string;
  applyRequires?: string[];
  artifacts?: ArtifactInfo[];
}

interface InstructionsInfo {
  context?: string;
  rules?: string;
  template?: string;
  instruction?: string;
  outputPath?: string;
  dependencies?: string[];
}

// ---------------------------------------------------------------------------
// Tool details stored in tool results (for rendering + state reconstruction)
// ---------------------------------------------------------------------------

interface OpenspecDetails {
  action: string;
  change?: string;
  output: string;
  error?: string;
  changes?: ChangeInfo[];
  status?: StatusInfo;
  instructions?: InstructionsInfo;
}

// ---------------------------------------------------------------------------
// Extension
// ---------------------------------------------------------------------------

export default function openspecExtension(pi: ExtensionAPI) {

  // -----------------------------------------------------------------------
  // Status footer — show active change + progress
  // -----------------------------------------------------------------------

  async function refreshStatus(ctx: ExtensionContext) {
    try {
      const out = await runOpenspec(pi, ctx.cwd, ["list", "--json"]);
      const changes: ChangeInfo[] = JSON.parse(out);
      const active = changes.filter(
        (c) => c.status !== "archived" && c.status !== "done",
      );

      if (active.length === 0) {
        ctx.ui.setStatus("openspec", undefined);
        return;
      }

      // Try to get task progress for the first active change
      let label = `⬡ ${active[0].name}`;
      try {
        const statusOut = await runOpenspec(pi, ctx.cwd, [
          "status",
          "--change",
          active[0].name,
          "--json",
        ]);
        const status: StatusInfo = JSON.parse(statusOut);
        if (status.artifacts) {
          const done = status.artifacts.filter((a) => a.status === "done").length;
          const total = status.artifacts.length;
          label = `⬡ ${active[0].name} [${done}/${total}]`;
        }
      } catch {
        // status failed, just show name
      }

      if (active.length > 1) {
        label += ` +${active.length - 1} more`;
      }
      ctx.ui.setStatus("openspec", label);
    } catch {
      // No changes or CLI not available — clear status
      ctx.ui.setStatus("openspec", undefined);
    }
  }

  // Refresh on session events
  pi.on("session_start", async (_event, ctx) => refreshStatus(ctx));
  pi.on("session_switch", async (_event, ctx) => refreshStatus(ctx));

  // Refresh after our tool runs
  pi.on("tool_result", async (event, ctx) => {
    if (event.toolName === "openspec") {
      // Small delay to let filesystem settle
      setTimeout(() => refreshStatus(ctx), 500);
    }
  });

  // -----------------------------------------------------------------------
  // Tool: openspec
  // -----------------------------------------------------------------------

  const OpenspecParams = Type.Object({
    action: StringEnum([
      "status",
      "list",
      "new",
      "instructions",
    ] as const),
    change: Type.Optional(
      Type.String({ description: "Change name (required for status, new, instructions)" }),
    ),
    artifact: Type.Optional(
      Type.String({
        description: "Artifact ID (for instructions action, e.g. 'proposal', 'design', 'tasks', 'apply')",
      }),
    ),
  });

  pi.registerTool({
    name: "openspec",
    label: "OpenSpec",
    description: `Manage OpenSpec specs and changes. Actions:
- status: Show status of a change (artifacts, progress). Requires 'change'.
- list: List all active changes.
- new: Create a new change directory. Requires 'change' (kebab-case name).
- instructions: Get artifact build instructions. Requires 'change' and 'artifact' (e.g. 'proposal', 'design', 'tasks', 'apply').

Output is truncated to ${DEFAULT_MAX_LINES} lines or ${formatSize(DEFAULT_MAX_BYTES)}.`,

    parameters: OpenspecParams,

    async execute(_toolCallId, params, signal, _onUpdate, ctx) {
      const { action, change, artifact } = params;

      try {
        let output: string;

        switch (action) {
          case "list": {
            output = await runOpenspec(pi, ctx.cwd, ["list", "--json"], signal);
            const changes: ChangeInfo[] = JSON.parse(output);
            const details: OpenspecDetails = {
              action,
              output,
              changes,
            };

            if (changes.length === 0) {
              return {
                content: [{ type: "text", text: "No active changes. Create one with action 'new'." }],
                details,
              };
            }

            const summary = changes
              .map((c) => `- ${c.name} (${c.status || "active"})`)
              .join("\n");
            return {
              content: [{ type: "text", text: `Active changes:\n${summary}` }],
              details,
            };
          }

          case "status": {
            if (!change) {
              return {
                content: [{ type: "text", text: "Error: 'change' parameter required for status action" }],
                details: { action, error: "missing change" } as OpenspecDetails,
              };
            }
            output = await runOpenspec(pi, ctx.cwd, ["status", "--change", change, "--json"], signal);
            const status: StatusInfo = JSON.parse(output);
            const details: OpenspecDetails = { action, change, output, status };

            // Build human-readable summary
            const lines: string[] = [`Change: ${change}`];
            if (status.schemaName) lines.push(`Schema: ${status.schemaName}`);
            if (status.artifacts) {
              lines.push("");
              lines.push("Artifacts:");
              for (const a of status.artifacts) {
                const icon = a.status === "done" ? "✓" : a.status === "ready" ? "○" : "·";
                lines.push(`  ${icon} ${a.id} (${a.status})`);
              }
              const done = status.artifacts.filter((a) => a.status === "done").length;
              lines.push("");
              lines.push(`Progress: ${done}/${status.artifacts.length}`);
            }
            if (status.applyRequires?.length) {
              lines.push(`Apply requires: ${status.applyRequires.join(", ")}`);
            }

            return {
              content: [{ type: "text", text: lines.join("\n") }],
              details,
            };
          }

          case "new": {
            if (!change) {
              return {
                content: [{ type: "text", text: "Error: 'change' parameter required for new action" }],
                details: { action, error: "missing change" } as OpenspecDetails,
              };
            }
            output = await runOpenspec(pi, ctx.cwd, ["new", "change", change], signal);
            return {
              content: [{ type: "text", text: `Created change: ${change}\n\n${output.trim()}` }],
              details: { action, change, output } as OpenspecDetails,
            };
          }

          case "instructions": {
            if (!change) {
              return {
                content: [{ type: "text", text: "Error: 'change' parameter required for instructions action" }],
                details: { action, error: "missing change" } as OpenspecDetails,
              };
            }
            if (!artifact) {
              return {
                content: [{ type: "text", text: "Error: 'artifact' parameter required for instructions action" }],
                details: { action, change, error: "missing artifact" } as OpenspecDetails,
              };
            }
            output = await runOpenspec(
              pi,
              ctx.cwd,
              ["instructions", artifact, "--change", change, "--json"],
              signal,
            );
            const instructions: InstructionsInfo = JSON.parse(output);
            const details: OpenspecDetails = { action, change, output, instructions };

            // Apply truncation
            const truncation = truncateHead(output, {
              maxLines: DEFAULT_MAX_LINES,
              maxBytes: DEFAULT_MAX_BYTES,
            });

            let resultText = truncation.content;
            if (truncation.truncated) {
              resultText += `\n\n[Output truncated: ${truncation.outputLines} of ${truncation.totalLines} lines`;
              resultText += ` (${formatSize(truncation.outputBytes)} of ${formatSize(truncation.totalBytes)})]`;
            }

            return {
              content: [{ type: "text", text: resultText }],
              details,
            };
          }

          default:
            return {
              content: [{ type: "text", text: `Unknown action: ${action}` }],
              details: { action, error: `unknown action: ${action}`, output: "" } as OpenspecDetails,
            };
        }
      } catch (err: any) {
        const message = err?.message || String(err);
        return {
          content: [{ type: "text", text: `OpenSpec error: ${message}` }],
          details: { action, change, error: message, output: "" } as OpenspecDetails,
          isError: true,
        };
      }
    },

    // -- Custom rendering --------------------------------------------------

    renderCall(args, theme) {
      let text = theme.fg("toolTitle", theme.bold("openspec "));
      text += theme.fg("accent", args.action);
      if (args.change) text += " " + theme.fg("muted", args.change);
      if (args.artifact) text += " " + theme.fg("dim", args.artifact);
      return new Text(text, 0, 0);
    },

    renderResult(result, { expanded }, theme) {
      const details = result.details as OpenspecDetails | undefined;

      if (details?.error) {
        return new Text(theme.fg("error", `✗ ${details.error}`), 0, 0);
      }

      if (!details) {
        const text = result.content[0];
        return new Text(text?.type === "text" ? text.text : "", 0, 0);
      }

      switch (details.action) {
        case "list": {
          const changes = details.changes || [];
          if (changes.length === 0) {
            return new Text(theme.fg("dim", "No active changes"), 0, 0);
          }
          let text = theme.fg("muted", `${changes.length} change(s)`);
          if (expanded) {
            for (const c of changes) {
              const status = c.status || "active";
              const icon = status === "done" ? theme.fg("success", "✓") : theme.fg("accent", "○");
              text += `\n  ${icon} ${theme.fg("text", c.name)} ${theme.fg("dim", `(${status})`)}`;
            }
          }
          return new Text(text, 0, 0);
        }

        case "status": {
          const status = details.status;
          if (!status?.artifacts) {
            return new Text(theme.fg("muted", `Change: ${details.change}`), 0, 0);
          }
          const done = status.artifacts.filter((a) => a.status === "done").length;
          const total = status.artifacts.length;
          const allDone = done === total;
          let text = allDone
            ? theme.fg("success", `✓ ${details.change} — all artifacts complete`)
            : theme.fg("muted", `${details.change} [${done}/${total}]`);

          if (expanded) {
            for (const a of status.artifacts) {
              const icon =
                a.status === "done"
                  ? theme.fg("success", "✓")
                  : a.status === "ready"
                    ? theme.fg("warning", "○")
                    : theme.fg("dim", "·");
              text += `\n  ${icon} ${theme.fg("text", a.id)} ${theme.fg("dim", `(${a.status})`)}`;
            }
          }
          return new Text(text, 0, 0);
        }

        case "new": {
          return new Text(
            theme.fg("success", "✓ Created ") + theme.fg("accent", details.change || "change"),
            0,
            0,
          );
        }

        case "instructions": {
          let text = theme.fg("success", "✓ Instructions ");
          text += theme.fg("dim", `(${details.change})`);
          if (!expanded) {
            text += " " + theme.fg("muted", "— expand to see full output");
          }
          return new Text(text, 0, 0);
        }

        default:
          return new Text(theme.fg("dim", details.output?.slice(0, 120) || ""), 0, 0);
      }
    },
  });

  // -----------------------------------------------------------------------
  // Command: /opsx — interactive dashboard
  // -----------------------------------------------------------------------

  pi.registerCommand("opsx", {
    description: "OpenSpec dashboard — view changes, specs, and status",
    getArgumentCompletions: (prefix) => {
      const subcommands = ["propose", "apply", "explore", "archive"];
      const filtered = subcommands.filter((s) => s.startsWith(prefix));
      return filtered.length > 0
        ? filtered.map((s) => ({ value: s, label: s }))
        : null;
    },
    handler: async (args, ctx) => {
      const sub = args.trim();

      // Sub-commands delegate to skills via sendUserMessage
      if (sub === "propose" || sub === "apply" || sub === "explore" || sub === "archive") {
        pi.sendUserMessage(`/skill:openspec-${sub === "propose" ? "propose" : sub === "apply" ? "apply-change" : sub === "explore" ? "explore" : "archive-change"}`, {
          deliverAs: "followUp",
        });
        return;
      }

      // Default: show dashboard
      try {
        const out = await runOpenspec(pi, ctx.cwd, ["list", "--json"]);
        const changes: ChangeInfo[] = JSON.parse(out);
        const active = changes.filter((c) => c.status !== "archived");

        if (active.length === 0) {
          ctx.ui.notify(
            "No active changes. Use /opsx propose or ask the agent to /opsx:propose",
            "info",
          );
          return;
        }

        // Build selection list
        const items = active.map(
          (c) => `${c.name} (${c.status || "active"})`,
        );

        const selected = await ctx.ui.select("OpenSpec Changes", items);
        if (!selected) return;

        const changeName = selected.split(" (")[0];

        // Show status for selected change
        try {
          const statusOut = await runOpenspec(pi, ctx.cwd, [
            "status",
            "--change",
            changeName,
          ]);
          ctx.ui.notify(statusOut.trim(), "info");
        } catch {
          ctx.ui.notify(`Change: ${changeName}`, "info");
        }
      } catch (err: any) {
        ctx.ui.notify(`OpenSpec: ${err.message || err}`, "error");
      }
    },
  });
}
