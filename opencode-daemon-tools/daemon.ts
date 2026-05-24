import { tool } from "@opencode-ai/plugin"

const helperBin = "agent-sandbox-helper"

async function runHelper(...args: string[]): Promise<string> {
  const result = await Bun.$`${helperBin} ${args}`.text()
  return result.trim()
}

export const git_pull = tool({
  description: "Git pull in a repository via the helper daemon",
  args: {
    path: tool.schema.string().describe("Absolute path to the git repository (defaults to current directory)").optional(),
  },
  async execute(args, context) {
    const path = args.path ?? context.directory
    const raw = await runHelper("git-pull", path)
    const parsed = JSON.parse(raw)
    if (parsed.ok) {
      let msg = `Git pull succeeded (exit code ${parsed.exit_code})`
      if (parsed.stdout) msg += `\nstdout:\n${parsed.stdout}`
      if (parsed.stderr) msg += `\nstderr:\n${parsed.stderr}`
      return msg
    }
    return `Git pull failed: ${parsed.error ?? parsed.stderr ?? raw}`
  },
})

export const git_push = tool({
  description: "Git push in a repository via the helper daemon (blocked on main/master branches)",
  args: {
    path: tool.schema.string().describe("Absolute path to the git repository (defaults to current directory)").optional(),
  },
  async execute(args, context) {
    const path = args.path ?? context.directory
    const raw = await runHelper("git-push", path)
    const parsed = JSON.parse(raw)
    if (parsed.ok) {
      let msg = `Git push succeeded (exit code ${parsed.exit_code})`
      if (parsed.stdout) msg += `\nstdout:\n${parsed.stdout}`
      if (parsed.stderr) msg += `\nstderr:\n${parsed.stderr}`
      return msg
    }
    return `Git push failed: ${parsed.error ?? parsed.stderr ?? raw}`
  },
})

export const pr_create = tool({
  description: "Create a pull request in Azure DevOps via the helper daemon",
  args: {
    path: tool.schema.string().describe("Absolute path to the git repository (defaults to current directory)").optional(),
    title: tool.schema.string().describe("PR title"),
    source: tool.schema.string().describe("Source branch name"),
    target: tool.schema.string().describe("Target branch (defaults to repository default)").optional(),
    description: tool.schema.string().describe("PR description / body text").optional(),
    work_item: tool.schema.number().describe("Azure DevOps work item ID to link after PR creation").optional(),
  },
  async execute(args, context) {
    const path = args.path ?? context.directory
    const cmdArgs = [
      "pr-create",
      "--path", path,
      "--title", args.title,
      "--source", args.source,
    ]
    if (args.target) cmdArgs.push("--target", args.target)
    if (args.description) cmdArgs.push("--description", args.description)
    if (args.work_item !== undefined) cmdArgs.push("--work-item", String(args.work_item))

    const result = await Bun.$`${helperBin} ${cmdArgs}`.text()
    const parsed = JSON.parse(result.trim())
    if (parsed.ok) {
      return `PR #${parsed.pull_request_id} created on ${parsed.provider}\nURL: ${parsed.url}`
    }
    return `PR creation failed: ${parsed.error}`
  },
})

export const dep_install = tool({
  description: "Install project dependencies via the helper daemon (auto-detects package manager from lockfile)",
  args: {
    path: tool.schema.string().describe("Absolute path to the project (defaults to current directory)").optional(),
  },
  async execute(args, context) {
    const path = args.path ?? context.directory
    const result = await Bun.$`${helperBin} dep-install ${path}`.text()
    const parsed = JSON.parse(result.trim())
    if (parsed.ok) {
      let msg = `Dependencies installed (${parsed.installed})`
      if (parsed.stdout) msg += `\nstdout:\n${parsed.stdout}`
      if (parsed.stderr) msg += `\nstderr:\n${parsed.stderr}`
      return msg
    }
    return `Dependency install failed: ${parsed.error}`
  },
})

export const wi_list = tool({
  description: "List Azure DevOps work items assigned to the current user via the helper daemon",
  args: {
    path: tool.schema.string().describe("Absolute path to the git repository (defaults to current directory)").optional(),
  },
  async execute(args, context) {
    const path = args.path ?? context.directory
    const result = await Bun.$`${helperBin} wi-list ${path}`.text()
    const parsed = JSON.parse(result.trim())
    if (parsed.ok) {
      const items = parsed.workitems ? JSON.parse(parsed.workitems) : []
      if (items.length === 0) return "No work items assigned to you."
      return items.map((wi: { id: number; title: string; state: string }) =>
        `#${wi.id} [${wi.state}] ${wi.title}`
      ).join("\n")
    }
    return `Failed to list work items: ${parsed.error}`
  },
})

export const wi_create = tool({
  description: "Create an Azure DevOps work item via the helper daemon",
  args: {
    path: tool.schema.string().describe("Absolute path to the git repository (defaults to current directory)").optional(),
    title: tool.schema.string().describe("Work item title"),
    type: tool.schema.string().describe("Work item type (defaults to Task)").optional(),
    parent: tool.schema.number().describe("Parent work item ID to link under").optional(),
    description: tool.schema.string().describe("Work item description").optional(),
  },
  async execute(args, context) {
    const path = args.path ?? context.directory
    const cmdArgs = [
      "wi-create",
      "--path", path,
      "--title", args.title,
    ]
    if (args.type) cmdArgs.push("--type", args.type)
    if (args.parent !== undefined) cmdArgs.push("--parent", String(args.parent))
    if (args.description) cmdArgs.push("--description", args.description)

    const result = await Bun.$`${helperBin} ${cmdArgs}`.text()
    const parsed = JSON.parse(result.trim())
    if (parsed.ok) {
      return `Work item created on ${parsed.provider}\n${JSON.stringify(JSON.parse(parsed.raw), null, 2)}`
    }
    return `Failed to create work item: ${parsed.error}`
  },
})