export const agentCalls = {
  scan: {
    name: "pgs_scan",
    arguments: { repo_path: "/repo", files: ["src/auth.rs"], full: true },
  },
  preview: {
    name: "pgs_stage",
    arguments: {
      repo_path: "/repo",
      selections: ["src/auth.rs:2-2"],
      dry_run: true,
      explain: true,
    },
  },
  stage: {
    name: "pgs_stage",
    arguments: { repo_path: "/repo", selections: ["src/auth.rs:2-2"] },
  },
  commit: {
    name: "pgs_commit",
    arguments: { repo_path: "/repo", message: "fix: reject blank tokens" },
  },
} as const;
