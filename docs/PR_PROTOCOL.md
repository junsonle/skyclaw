# TEMM1E Pull Request Protocol

**Every pull request to TEMM1E must follow this protocol. No exceptions.**

TEMM1E is a live production system with real users. It is built on extreme resilience — zero panic paths, zero warnings, 4-layer panic defense, automatic recovery. Your PR must maintain this standard. A merged bug doesn't just fail a test — it kills a user's bot.

---

## Before You Start

### 1. Understand What You Cannot Touch

**Agentic Core is LOCKED.** Only the project founder may modify these paths:

| Protected Path | Contains |
|---|---|
| `crates/temm1e-agent/` | Agent runtime, context, executor, budget, classification, streaming, self-correction, task decomposition, prompt optimization, consciousness, learning, recovery, watchdog, circuit breaker |
| `crates/temm1e-core/src/traits/` | All shared trait definitions (`Channel`, `Provider`, `Memory`, `Tool`, `Vault`, etc.) |
| `crates/temm1e-core/src/types/` | Shared types, error enum (`Temm1eError`), config schema |
| `src/main.rs` | CLI entry point, gateway router, worker dispatch, onboarding flow |

If your change requires modifications to any of these paths, **stop and open an issue instead.** Describe what you need changed and why. The founder will evaluate and implement it, or provide an alternative approach.

PRs that touch protected paths will be closed without review.

### 2. Understand the Zero-Risk Policy

Every PR must demonstrate **ZERO RISK** to the existing system:

- **Zero behavioral changes** to existing features unless explicitly fixing a documented bug
- **Zero new panic paths** — no `.unwrap()` or `.expect()` in production code
- **Zero new warnings** — `cargo clippy` must pass with `-D warnings`
- **Zero cross-crate violations** — leaf crates never depend on each other
- **Zero unconditional SDK imports** — feature-gate all optional dependencies
- **Zero security regressions** — empty allowlists deny all, numeric IDs only, path sanitization, key redaction

If you cannot prove zero risk, your PR is not ready. Open an issue or discussion first.

### 3. Identify Your Change Category

| Category | Scope | Examples |
|---|---|---|
| **Provider** | `crates/temm1e-providers/` | New AI provider, fix provider parsing, add streaming support |
| **Channel** | `crates/temm1e-channels/` | New messaging channel, fix message handling |
| **Tool** | `crates/temm1e-tools/` | New agent tool, fix tool execution |
| **Memory** | `crates/temm1e-memory/` | New memory backend, fix persistence |
| **Cognitive** | `crates/temm1e-{anima,hive,distill,perpetuum,gaze}/` | Cognitive system changes (high scrutiny) |
| **Docs** | `docs/`, `README.md` | Documentation only |
| **CI/Infra** | `.github/`, `Dockerfile`, `install.sh` | Build and deployment |

---

## Development Requirements

### 4. Architecture Rules

These are non-negotiable. PRs that violate any of these will be rejected:

1. **Traits in core, implementations in crates.** If you need a new shared trait, that requires a core change (founder-only). Use existing traits.
2. **No cross-implementation dependencies.** Your provider crate cannot import your channel crate. Shared types go through `temm1e-core`.
3. **Feature flags for optional dependencies.** Never `use some_sdk;` unconditionally. Gate behind `#[cfg(feature = "...")]`.
4. **Factory pattern.** Expose `create_*()` functions that dispatch by name string. Return `Box<dyn Trait>`.
5. **All errors are `Temm1eError`.** Use the appropriate variant. Never introduce new error types outside core.
6. **`#[async_trait]` for all async trait impls.** All trait objects must be `Send + Sync`.

### 5. Code Quality Standards

```bash
# ALL FOUR must pass. No exceptions.
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
```

- **Zero warnings.** Clippy warnings are errors.
- **Zero panic paths.** No `.unwrap()` or `.expect()` on anything that could fail at runtime. Use `?` with `Temm1eError`.
- **UTF-8 safety.** Never use `&text[..N]` on user-provided strings. Use `char_indices()` to find safe boundaries.
- **Structured logging.** Use `tracing::{debug,info,warn,error}!` with named fields. Never log API keys or tokens at info level.
- **No `max_tokens` hardcoding.** Never hardcode `max_tokens` on LLM output. Set `None` always.

### 6. Security Checklist

Every PR must self-verify against these rules:

| Rule | Pattern |
|---|---|
| Empty allowlist = deny all | `if list.is_empty() { return false; }` |
| Match numeric user IDs only | Never match on username strings |
| Sanitize filenames | `Path::new(&name).file_name().unwrap_or("unnamed")` |
| Validate resolved paths | `full.starts_with(&self.base_dir)` |
| Redact keys in Debug | Custom `fmt::Debug`: `sk-an...xyz9` |
| Tools declare resources | `ToolDeclarations { file_access, network_access, shell_access }` |
| Delete credential messages | `channel.delete_message()` after reading keys/passwords |

### 7. Unit Tests

Every new module and function must have tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_happy_path() { /* ... */ }

    #[tokio::test]
    async fn test_error_case() { /* ... */ }

    #[tokio::test]
    async fn test_edge_case() { /* ... */ }
}
```

- Use `#[tokio::test]` for async tests
- SQLite tests: `SqliteMemory::new("sqlite::memory:")`
- File tests: `tempfile::tempdir()`
- Test the full CRUD cycle where applicable
- Test edge cases: empty inputs, boundary values, malformed data

---

## Mandatory: Live CLI Chat Test

**Unit tests are necessary but not sufficient.** TEMM1E is an LLM application — the agent loop, context management, provider interaction, and conversation flow cannot be fully validated by unit tests alone. Deterministic tests verify structure; live tests verify behavior.

**Every PR that touches runtime code must include a live CLI chat test with logs as proof.**

### 8. Running the Live Test

Build and run a multi-turn conversation through the CLI chat interface:

```bash
# 1. Build release binary
cargo build --release --bin temm1e

# 2. Reset to fresh state
rm -f ~/.temm1e/memory.db

# 3. Source environment (your own API key)
export ANTHROPIC_API_KEY="your-key"

# 4. Write test script
cat > /tmp/temm1e_pr_test.sh << 'SCRIPT'
#!/bin/bash
(
  echo "Turn 1: Hello, what model are you using?"
  sleep 15
  echo "Turn 2: What is 42 * 17?"
  sleep 15
  echo "Turn 3: Write a haiku about Rust programming"
  sleep 15
  echo "Turn 4: What was my first question?"
  sleep 15
  echo "Turn 5: Summarize our conversation"
  sleep 15
  echo "/quit"
) | ./target/release/temm1e chat 2>&1
SCRIPT

# 5. Run (capture full output)
bash /tmp/temm1e_pr_test.sh | tee /tmp/temm1e_pr_test_output.log
```

**Minimum 5 turns.** Adjust the conversation to exercise your specific change:

| Change Type | Test Focus |
|---|---|
| Provider fix | Multi-turn with the affected provider, verify responses parse correctly |
| Channel fix | Test through the affected channel if possible, otherwise CLI |
| Tool change | Include turns that trigger the tool, verify tool output in conversation |
| Memory change | Include a recall turn (Turn 4 pattern: "What was my first question?") |
| Streaming fix | Verify responses stream incrementally, not in one block |

### 9. What the Logs Must Show

Your test output must demonstrate:

- [ ] All turns received responses (no silent failures)
- [ ] No panics, no `ERROR` level log lines
- [ ] Conversation memory works (later turns can reference earlier ones)
- [ ] Budget tracking increments (cost accumulates across turns)
- [ ] If your change is provider-related: the correct provider was used
- [ ] If your change is tool-related: tool calls executed and returned results
- [ ] Clean shutdown on `/quit`

### 10. Attaching Proof to the PR

Include the test output in your PR description under a collapsible section:

```markdown
## Live CLI Chat Test

**Turns:** 5
**Provider:** Anthropic (Claude Sonnet 4)
**Result:** All turns responded, memory recall confirmed, zero errors

<details>
<summary>Full test output (click to expand)</summary>

```
[paste /tmp/temm1e_pr_test_output.log contents here]
```

</details>
```

**PRs without live test logs will not be reviewed** (exception: docs-only and CI-only changes).

---

## PR Submission Format

### 11. Branch Naming

```
fix/<scope>-<description>       # Bug fixes
feat/<scope>-<description>      # New features
docs/<description>              # Documentation only
ci/<description>                # CI/infrastructure
```

Examples: `fix/anthropic-unknown-blocks`, `feat/channels-matrix`, `docs/pr-protocol`

### 12. Commit Messages

Use conventional commits:

```
<type>(<scope>): <short description>

<body — explain WHY, not just WHAT>

Co-Authored-By: <name> <email>
```

Types: `fix`, `feat`, `docs`, `style`, `refactor`, `test`, `ci`

### 13. PR Description Template

```markdown
## Summary

- [1-3 bullet points: what changed and why]

## Problem

[What was broken or missing? Link to issue if applicable.]

## Fix / Implementation

[What you changed. Be specific — name the enums, functions, files.]

## Risk Assessment

- **Files changed:** [list]
- **Agentic Core touched:** No (REQUIRED — if yes, PR will be closed)
- **Behavioral changes to existing features:** None / [describe]
- **New panic paths:** None
- **New dependencies:** None / [list with justification]
- **Security implications:** None / [describe]
- **Backwards compatibility:** Full / [describe migration]

## Compilation Gates

```
cargo check    — PASS
cargo clippy   — PASS (0 warnings)
cargo fmt      — PASS
cargo test     — PASS (N tests, 0 failures)
```

## Live CLI Chat Test

**Turns:** N
**Provider:** [which provider]
**Result:** [summary]

<details>
<summary>Full test output</summary>

[paste logs]

</details>

## Test Plan

- [x] Unit tests added/updated
- [x] Compilation gates pass
- [x] Live CLI chat test attached
- [x] No changes to Agentic Core
- [x] Zero new warnings
- [x] Zero new panic paths
```

---

## Review Criteria

Reviewers (and the founder) will evaluate PRs against:

| Gate | Requirement |
|---|---|
| **Agentic Core** | Zero modifications to protected paths |
| **Zero Risk** | No behavioral changes to existing features |
| **Compilation** | All 4 gates pass |
| **Tests** | Unit tests for new code, existing tests still pass |
| **Live Test** | CLI chat logs attached showing zero errors |
| **Security** | Checklist verified, no new attack surface |
| **Architecture** | No cross-crate violations, traits in core, factory pattern |
| **Code Quality** | No unwrap, no warnings, structured logging, UTF-8 safe |
| **Formatting** | `cargo fmt` clean (CI will reject otherwise) |

**One failure = PR blocked.** Fix and re-push.

---

## FAQ

**Q: My change is tiny (one line). Do I still need live tests?**
A: If it touches runtime code (anything under `crates/` that isn't docs), yes. PR #30 was a "tiny" serde fix — it still needed `cargo fmt` and would have benefited from a live test. Small changes cause production outages too.

**Q: I need a new trait or error variant. What do I do?**
A: Open an issue describing what you need. Core and trait changes are founder-only because they affect every crate in the workspace.

**Q: Can I add a new crate?**
A: Open an issue first. New crates must be justified, follow the factory pattern, and integrate with the existing architecture. The founder will scaffold it or approve the structure.

**Q: My PR failed CI on `cargo fmt`. Can I just push a fix commit?**
A: Yes. Always run `cargo fmt --all -- --check` locally before pushing.

**Q: What if I disagree with a review decision?**
A: Open a discussion. The founder makes final calls on architecture and risk, but reasoning is always welcome.
