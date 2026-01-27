# Vercel AI SDK Schemas

This package extracts JSON Schema for `UIMessage` from the Vercel AI SDK v6 TypeScript types.

## Usage

- Install dependencies in this folder.
- Run the extractor:

```
pnpm install
pnpm extract
```

Optional flags:
- `--version=6.x.y` to pin an exact version
- `--major=6` to select the latest version for a major (default: 6)

Output:
- `artifacts/json-schema/ui-message.json`

The registry response is cached under `.cache/` for 24 hours. The extractor downloads the AI SDK package
and the minimal dependency tree needed for TypeScript type resolution into a temporary folder.
