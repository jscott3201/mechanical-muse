import type { ToolCallView } from "@/types/generated/ToolCallView";

/**
 * One of Spark's own checks in words, e.g. "Spark replayed scenario x: failed at 240 s, y
 * expected 285.93, got 291.48". Built from the server's one-line summary (mm-agent's tools).
 */
export function checkText(call: ToolCallView): string {
  switch (call.name) {
    case "replay_scenario": {
      const m = /^scenario (\S+) (.*)$/.exec(call.summary);
      if (m) return `Spark replayed scenario ${m[1] ?? ""}: ${(m[2] ?? "").replace(": ", ", ")}`;
      return `Spark tried to replay a scenario: ${call.summary}`;
    }
    case "check_graph":
      return `Spark checked its draft: ${call.summary}`;
    case "describe_block":
      return call.ok ? `Spark looked up ${call.args}` : `Spark looked up ${call.args}: ${call.summary}`;
    default:
      return `Spark called ${call.name}: ${call.summary}`;
  }
}
