## launch

- re-review agent schemas and compare it to ours
- auto-serve frontend from cli
- verify embedded sdk works
- fix bugs in ui
    - double messages
    - user-sent messages
    - permissions
- consider migraing our standard to match the vercel ai standard
- discuss actor arch in readme + give example
- skillfile
    - specifically include the release checklist

## soon

- **Vercel AI SDK Compatibility**: Works with existing AI SDK tooling, like `useChat`
- **Auto-configure MCP & Skills**: Auto-load MCP servers & skills for your agents
- **Process & logs manager**: Manage processes, logs, and ports for your agents to run background processes

## later

- review all flags available on coding agents clis
- set up agent to check diffs in versions to recommend updates
- auto-updating for long running job
- persistence
- system information/cpu/etc
- api features
    - list agent modes available
    - list models available
    - handle planning mode
- api key gateway
- configuring mcp/skills/etc
- process management inside container
- otel
- better authentication systems
- s3-based file system
- ai sdk compatibility for their ecosystem (useChat, etc)
- resumable messages
- todo lists
- all other features
- misc
    - bootstrap tool that extracts tokens from the current system
- skill
- pre-package these as bun binaries instead of npm installations
- build & release pipeline with musl
- agent feature matrix for api features
- tunnels
- mcp integration (can connect to given endpoints)
- provide a pty to access the agent data
- other agent features like file system
- python sdk
- comparison to agentapi:
    - it does not use the pty since we need to get more information from the agent directly
- transfer sessions between agents
