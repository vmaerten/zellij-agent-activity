// opencode plugin → zellij pipe bridge. Drop this file in ~/.config/opencode/plugins/.
//
// opencode has no command hooks, so this producer is a program rather than the
// forwarder.sh the other two use. Everything lives in one file on purpose: the
// install is a single copy, and the loader treats *every* export as a plugin
// function, so there is nothing else to export (ADR-0011).

import { spawn } from "node:child_process"
import { appendFile, mkdir } from "node:fs/promises"
import { dirname } from "node:path"

const VERSION = "0.1.0"
const PIPE_NAME = "agent_activity.v1"
const PIPE_TIMEOUT_MS = 5000

// opencode names its tools its own way; the wire carries the canonical
// vocabulary and the producer translates into it, so the wasm never grows a
// harness branch (ADR-0010). Unlisted names ride through and render as the
// generic tool symbol, which is where `todowrite`, `skill` and every MCP tool
// land. The ids are the runtime ones, read from `GET /experimental/tool/ids`.
const TOOL_NAMES = new Map([
  ["bash", "Bash"],
  ["edit", "Edit"],
  ["apply_patch", "Edit"],
  ["write", "Write"],
  ["read", "Read"],
  ["glob", "Glob"],
  ["grep", "Grep"],
  ["task", "Agent"],
  ["webfetch", "WebFetch"],
  ["websearch", "WebSearch"],
])

// Never dump tool arguments: they can be large and can hold secrets. The raw
// opencode names are kept, though: they are what a mapping bug is read from.
const trace = async (entry) => {
  const path = process.env.ZELLIJ_AGENT_ACTIVITY_LOG
  if (!path) return
  try {
    await mkdir(dirname(path), { recursive: true })
    await appendFile(path, JSON.stringify({ at: new Date().toISOString(), v: VERSION, ...entry }) + "\n")
  } catch {
    // A debug log that breaks the agent is worse than no debug log.
  }
}

export const ZellijAgentActivity = async () => {
  const paneId = process.env.ZELLIJ_PANE_ID
  if (!process.env.ZELLIJ_SESSION_NAME || !paneId) return {}

  // opencode has no turn boundary telling the main agent from a subagent, so a
  // subagent's `session.idle` would post a premature ✓. Only sessions seen
  // being born with a parent are ignored; an unknown one is the resumed root
  // session, created before this plugin loaded (ADR-0007).
  const children = new Set()

  const send = (source, sessionID, hookEvent, { tool = "", notification = "", rawTool } = {}) => {
    const tsMs = Date.now()
    const dropped = sessionID !== undefined && children.has(sessionID)
    let args = null
    if (!dropped) {
      args = `pane_id=${paneId},hook_event=${hookEvent},tool_name=${tool},ts_ms=${tsMs}`
      if (notification) args += `,notification=${notification}`
    }
    trace({ ts_ms: tsMs, pane_id: paneId, source, session_id: sessionID, tool: rawTool, dropped: dropped || undefined, args })
    if (!args) return Promise.resolve()
    if (process.env.ZELLIJ_AGENT_ACTIVITY_DRY_RUN) {
      process.stdout.write(args + "\n")
      return Promise.resolve()
    }
    // `zellij pipe` blocks until the plugin consumes the message, so without
    // this timeout a stuck plugin leaks a file descriptor per event until the
    // zellij server hits EMFILE and crashes. The promise never rejects, and
    // only `dispose` awaits it: elsewhere the agent must not wait on us, and
    // `ts_ms` keeps the wasm's ordering safe.
    //
    // `stdio: "ignore"` is load-bearing, not tidiness: `zellij pipe` also reads
    // a payload from stdin and blocks until EOF, so an inherited pipe hangs it
    // until the timeout kills it and the message is never delivered.
    return new Promise((resolve) => {
      const child = spawn("zellij", ["pipe", "--name", PIPE_NAME, "--args", args], {
        stdio: "ignore",
        timeout: PIPE_TIMEOUT_MS,
      })
      const failed = (why) => {
        trace({ ts_ms: tsMs, pane_id: paneId, source, args, failed: why })
        resolve()
      }
      child.on("error", (err) => failed(err.message))
      child.on("close", (code, signal) => (code === 0 ? resolve() : failed(`zellij pipe exited ${signal ?? code}`)))
    })
  }

  // Nothing is sent on load: opencode instantiates the plugin more than once,
  // and an instance born during shutdown would leave a ◆ behind after the
  // `dispose` that cleared the pane. `session.created` is the honest signal.
  return {
    event: async ({ event }) => {
      const props = event.properties ?? {}
      switch (event.type) {
        case "session.created": {
          const info = props.info ?? {}
          // Registered before sending, so the subagent's own birth is the first
          // thing the trace shows being dropped.
          if (info.parentID) children.add(info.id)
          send(event.type, info.id, "SessionStart")
          return
        }
        case "session.idle":
          send(event.type, props.sessionID, "Stop")
          return
        // `permission.updated` is what releases before 1.18 called it. Both are
        // published only once the request has survived the allow rules, so this
        // never fires for a permission the config grants on its own.
        case "permission.asked":
        case "permission.updated":
          send(event.type, props.sessionID ?? props.info?.sessionID, "Notification", { notification: "permission" })
          return
        case "permission.replied":
          send(event.type, props.sessionID, "PostToolUse")
          return
      }
    },

    "chat.message": async ({ sessionID }) => {
      send("chat.message", sessionID, "UserPromptSubmit")
    },

    "tool.execute.before": async ({ tool, sessionID }) => {
      // An agent stopping to ask you a question is the very thing ⚠ exists for.
      // Its `tool.execute.after` carries the answer and clears it.
      if (tool === "question") {
        send("tool.execute.before", sessionID, "Notification", { notification: "permission", rawTool: tool })
        return
      }
      send("tool.execute.before", sessionID, "PreToolUse", { tool: TOOL_NAMES.get(tool) ?? tool, rawTool: tool })
    },

    "tool.execute.after": async ({ tool, sessionID }) => {
      send("tool.execute.after", sessionID, "PostToolUse", { rawTool: tool })
    },

    dispose: async () => {
      await send("dispose", undefined, "SessionEnd")
    },
  }
}
