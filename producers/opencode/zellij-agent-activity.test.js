// Tests the plugin: given an opencode hook or event, are the pipe args right?
// Driven through the real hooks, with the same dry-run seam the shell producers
// use — here it prints the args instead of piping them.

import assert from "node:assert/strict"
import test from "node:test"

import { ZellijAgentActivity } from "./zellij-agent-activity.js"

process.env.ZELLIJ_SESSION_NAME = "test"
process.env.ZELLIJ_PANE_ID = "7"
process.env.ZELLIJ_AGENT_ACTIVITY_DRY_RUN = "1"
delete process.env.ZELLIJ_AGENT_ACTIVITY_LOG

const captured = []
const stdout = process.stdout.write.bind(process.stdout)
process.stdout.write = (chunk, ...rest) => {
  const line = String(chunk)
  if (line.startsWith("pane_id=")) {
    captured.push(line.trim())
    return true
  }
  return stdout(chunk, ...rest)
}

const load = async () => await ZellijAgentActivity()

const drain = () => captured.splice(0)

const only = () => {
  const lines = drain()
  assert.equal(lines.length, 1, `expected one pipe, got ${JSON.stringify(lines)}`)
  return lines[0]
}

const preTool = async (hooks, tool, sessionID = "ses_root") => {
  await hooks["tool.execute.before"]({ tool, sessionID, callID: "call_1" }, { args: {} })
}

const emit = async (hooks, type, properties) => {
  await hooks.event({ event: { type, properties } })
}

// opencode instantiates the plugin more than once, so loading must stay silent:
// an instance born during shutdown would leave a prefix behind (ADR-0011).
test("loading says nothing on its own", async () => {
  await load()
  assert.deepEqual(drain(), [])
})

test("outside Zellij the plugin registers nothing", async () => {
  const pane = process.env.ZELLIJ_PANE_ID
  delete process.env.ZELLIJ_PANE_ID
  try {
    assert.deepEqual(await ZellijAgentActivity(), {})
    assert.deepEqual(drain(), [])
  } finally {
    process.env.ZELLIJ_PANE_ID = pane
  }
})

test("a user prompt starts a turn", async () => {
  const hooks = await load()
  await hooks["chat.message"]({ sessionID: "ses_root" }, { message: {}, parts: [] })
  assert.match(only(), /hook_event=UserPromptSubmit,tool_name=,/)
})

const translations = [
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
]

for (const [tool, wire] of translations) {
  test(`${tool} -> ${wire}`, async () => {
    const hooks = await load()
    await preTool(hooks, tool)
    assert.match(only(), new RegExp(`hook_event=PreToolUse,tool_name=${wire},`))
  })
}

test("an untranslated tool rides through", async () => {
  const hooks = await load()
  await preTool(hooks, "todowrite")
  assert.match(only(), /hook_event=PreToolUse,tool_name=todowrite,/)
})

test("the question tool raises the warning instead of a tool", async () => {
  const hooks = await load()
  await preTool(hooks, "question")
  const line = only()
  assert.match(line, /hook_event=Notification,tool_name=,/)
  assert.match(line, /notification=permission$/)
  assert.doesNotMatch(line, /PreToolUse/)
})

test("a finished tool goes back to thinking", async () => {
  const hooks = await load()
  await hooks["tool.execute.after"]({ tool: "bash", sessionID: "ses_root", callID: "c", args: {} }, {})
  const line = only()
  assert.match(line, /hook_event=PostToolUse,tool_name=,/)
  assert.doesNotMatch(line, /notification=/)
})

for (const type of ["permission.asked", "permission.updated"]) {
  test(`${type} needs the user`, async () => {
    const hooks = await load()
    await emit(hooks, type, { sessionID: "ses_root", id: "per_1" })
    assert.match(only(), /hook_event=Notification,tool_name=,ts_ms=\d+,notification=permission$/)
  })
}

test("answering a permission clears the warning", async () => {
  const hooks = await load()
  await emit(hooks, "permission.replied", { sessionID: "ses_root", permissionID: "per_1", response: "once" })
  assert.match(only(), /hook_event=PostToolUse,/)
})

test("an idle session ends the turn", async () => {
  const hooks = await load()
  await emit(hooks, "session.idle", { sessionID: "ses_root" })
  const line = only()
  assert.match(line, /hook_event=Stop,tool_name=,/)
  assert.doesNotMatch(line, /notification=/)
})

test("a new root session announces itself", async () => {
  const hooks = await load()
  await emit(hooks, "session.created", { info: { id: "ses_other" } })
  assert.match(only(), /^pane_id=7,hook_event=SessionStart,tool_name=,ts_ms=\d+$/)
})

test("a subagent session is born silent and stays silent", async () => {
  const hooks = await load()
  await emit(hooks, "session.created", { info: { id: "ses_child", parentID: "ses_root" } })
  assert.deepEqual(drain(), [])

  await preTool(hooks, "bash", "ses_child")
  await hooks["tool.execute.after"]({ tool: "bash", sessionID: "ses_child", callID: "c", args: {} }, {})
  await emit(hooks, "session.idle", { sessionID: "ses_child" })
  await emit(hooks, "permission.asked", { sessionID: "ses_child" })
  assert.deepEqual(drain(), [])
})

test("a session nobody saw being born is treated as the root one", async () => {
  const hooks = await load()
  await emit(hooks, "session.idle", { sessionID: "ses_resumed" })
  assert.match(only(), /hook_event=Stop,/)
})

test("an unknown event changes nothing", async () => {
  const hooks = await load()
  await emit(hooks, "todo.updated", { sessionID: "ses_root" })
  assert.deepEqual(drain(), [])
})

test("disposing clears the prefix", async () => {
  const hooks = await load()
  await hooks.dispose()
  assert.match(only(), /hook_event=SessionEnd,tool_name=,/)
})
