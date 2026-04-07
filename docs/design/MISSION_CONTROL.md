# Mission Control — Interceptor v2 for Perpetuum Harmony

**Supersedes:** `INTERCEPTOR_ROADMAP.md` Phases 2-5
**Status:** Design complete, pending implementation
**Prerequisite:** Phase 1 already shipped (AgentTaskStatus + CancellationToken)

---

## 1. Problem Statement

User feedback: the interceptor does not inform what the main agent is doing. It used to work nicely before Perpetuum was integrated.

The interceptor was designed for a **single-task model** — one worker processes one user request. With Perpetuum, Tem works on multiple things concurrently: user tasks, heartbeats, monitors, volition actions, scheduled work. The interceptor is blind to this reality.

### Specific Failures

| Issue | Root Cause | Impact |
|-------|-----------|--------|
| Interceptor gives vague status | `current_task` is raw user message text, no phase info | User asks "what are you doing?" and gets their own message parroted back |
| `_status_rx` dropped | `main.rs:2697` creates watch channel but drops receiver | 13 phase emissions in runtime.rs go to /dev/null |
| Heartbeat poisons current_task | Heartbeat text overwrites `current_task` at `main.rs:4191` | User sees "I'm working on HEARTBEAT — You are running autonomously..." |
| No Perpetuum awareness | Interceptor doesn't query active concerns | User can't see monitors, alarms, or scheduled work |
| Can't accept new orders | All mid-task messages go to pending queue (tool result injection) | "Check my email" gets stuffed into current Python script task |
| CancellationToken stays cancelled | Token created once per slot (`main.rs:2595`), never refreshed | After `/stop`, next task may inherit a dead token |

---

## 2. Root Cause Analysis

### The Dispatch Flow (main.rs lines 2408-2565)

```
User msg arrives via msg_rx.recv()
  │
  ├─ is_heartbeat_msg? → skip Perpetuum recording
  ├─ existing slot? → check preemption, /stop, is_busy
  │   │
  │   ├─ is_busy == true:
  │   │   ├─ Push text to pending queue (lines 2460-2465)  ← UNCONDITIONAL
  │   │   ├─ Spawn LLM interceptor (lines 2467-2562)
  │   │   └─ continue (skip sending to worker channel)
  │   │
  │   └─ is_busy == false:
  │       └─ Fall through → send to worker channel
  │
  └─ no slot → create worker, send to channel
```

**Problem 1:** The pending queue push happens BEFORE the interceptor classifies. The agent may consume the message as an "amendment" (tool result injection at `runtime.rs:1942`) before the interceptor even finishes its LLM call. There's no way to route a message as a "new order" vs "amendment" because the routing happens unconditionally.

**Problem 2:** The interceptor prompt only receives `current_task` (raw text) with no phase context:
```
Current task: "{raw_user_message_text}"
```
vs what it COULD receive:
```
Phase: Running shell_exec (1/3, round 2)
Elapsed: 12s | Rounds: 2 | Cost: $0.0023
```

**Problem 3:** `current_task` is set for ALL messages including heartbeats (`main.rs:4190-4192`):
```rust
if let Ok(mut ct) = current_task_clone.lock() {
    *ct = msg.text.as_deref().unwrap_or("").to_string();
}
```
No guard for `is_hb`.

---

## 3. Current Code References

### ChatSlot struct (main.rs:2368-2375)
```rust
struct ChatSlot {
    tx: tokio::sync::mpsc::Sender<InboundMessage>,    // worker channel (capacity 4)
    interrupt: Arc<AtomicBool>,                        // external interrupt flag
    is_heartbeat: Arc<AtomicBool>,                     // true during heartbeat processing
    is_busy: Arc<AtomicBool>,                          // true during any processing
    current_task: Arc<std::sync::Mutex<String>>,       // raw message text
    cancel_token: tokio_util::sync::CancellationToken, // created once per slot
}
```

### Slot creation (main.rs:2587-2599)
```rust
let slot = slots.entry(chat_id.clone()).or_insert_with(|| {
    let (chat_tx, mut chat_rx) = tokio::sync::mpsc::channel::<InboundMessage>(4);
    let interrupt = Arc::new(AtomicBool::new(false));
    let is_heartbeat = Arc::new(AtomicBool::new(false));
    let is_busy = Arc::new(AtomicBool::new(false));
    let current_task: Arc<std::sync::Mutex<String>> = Arc::new(std::sync::Mutex::new(String::new()));
    let cancel_token = tokio_util::sync::CancellationToken::new();
    // ... worker tokio::spawn follows ...
});
```

### Status watch creation (main.rs:2694-2700) — per-message, receiver dropped
```rust
// ── Phase 1: status watch + cancel token ──────
let (status_tx, _status_rx) = tokio::sync::watch::channel(
    temm1e_agent::AgentTaskStatus::default(),
);
let cancel = cancel_token_clone.clone();
```

### current_task set (main.rs:4189-4192) — no heartbeat guard
```rust
is_busy_clone.store(true, Ordering::Relaxed);
if let Ok(mut ct) = current_task_clone.lock() {
    *ct = msg.text.as_deref().unwrap_or("").to_string();
}
```

### Pending queue type
```rust
// crates/temm1e-agent/src/runtime.rs:55
pub type PendingMessages = Arc<std::sync::Mutex<HashMap<String, Vec<String>>>>;
```
Keyed by chat_id, stores `Vec<String>` (text only, no message IDs).

### Pending consumption (runtime.rs:1940-1966)
```rust
if let Some(ref pq) = pending {
    if let Ok(mut map) = pq.lock() {
        if let Some(msgs) = map.remove(&msg.chat_id) {  // DESTRUCTIVE — removes all
            // ... format and inject into last ToolResult content part
        }
    }
}
```
Uses `map.remove()` — destructive consumption. Messages injected as:
```
[PENDING MESSAGES — the user sent new message(s) while you were working.
 Acknowledge with send_message and decide: finish current task or stop and respond.]
  1. "message text"
  2. "message text"
```

### Pending drain after task completion (main.rs:4765-4793)
```rust
if let Ok(mut pq) = pending_for_worker.lock() {
    if let Some(pending_msgs) = pq.remove(&worker_chat_id) {
        for text in pending_msgs {
            let synthetic = InboundMessage { id: uuid::Uuid::new_v4().to_string(), ... };
            self_tx.try_send(synthetic); // re-queue into worker's own channel
        }
    }
}
is_heartbeat_clone.store(false, Ordering::Relaxed);
is_busy_clone.store(false, Ordering::Relaxed);
```

### process_message signature (runtime.rs:354-362)
```rust
pub async fn process_message(
    &self,
    msg: &InboundMessage,
    session: &mut SessionContext,
    interrupt: Option<Arc<AtomicBool>>,
    pending: Option<PendingMessages>,
    reply_tx: Option<tokio::sync::mpsc::UnboundedSender<OutboundMessage>>,
    status_tx: Option<tokio::sync::watch::Sender<AgentTaskStatus>>,
    cancel: Option<CancellationToken>,
) -> Result<(OutboundMessage, TurnUsage), Temm1eError>
```

### All process_message call sites in main.rs

| Line | Context | interrupt | pending | reply_tx | status_tx | cancel |
|------|---------|-----------|---------|----------|-----------|--------|
| 4201 | Main worker | Some | Some | Some | Some | Some |
| 4335 | Hive mini-agent | None | None | None | None | None |
| 4422 | Fallback agent | Some | Some | None | **None** | Some |
| 6268 | CLI chat | None | None | Some | **None** | None |
| 6347 | CLI non-hive | None | None | None | **None** | None |

### AgentTaskPhase variants (agent_task_status.rs:16-36)
```rust
pub enum AgentTaskPhase {
    Preparing,
    Classifying,
    CallingProvider { round: u32 },
    ExecutingTool { round: u32, tool_name: String, tool_index: u32, tool_total: u32 },
    Finishing,
    Done,
    Interrupted { round: u32 },
}
```

### AgentTaskStatus fields (agent_task_status.rs:43-58)
```rust
pub struct AgentTaskStatus {
    pub phase: AgentTaskPhase,
    pub started_at: Instant,
    pub rounds_completed: u32,
    pub tools_executed: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cost_usd: f64,
}
```

### Status emission points in runtime.rs
10+ `send_modify` calls at: lines 439, 562, 935, 1151, 1226, 1373, 1728, 1787, 2038, 2050.

### Perpetuum temporal context
```rust
// main.rs:1979-1980
let perpetuum_temporal: Arc<tokio::sync::RwLock<String>> = Arc::new(tokio::sync::RwLock::new(String::new()));
```
Updated on every user interaction (main.rs:2414-2417):
```rust
if let Some(ref perp) = *perpetuum.read().await {
    perp.record_user_interaction().await;
    let temporal = perp.temporal_injection("standard").await;
    *perpetuum_temporal.write().await = temporal;
}
```
Contains: active concerns summary, next event, conscience state, parked tasks.

### TUI reference implementation (temm1e-tui/src/lib.rs:197-221)
```rust
let status_changed = async {
    if let Some(ref mut handle) = agent_handle {
        handle.status_rx.changed().await.ok();
        Some(handle.status_rx.borrow().clone())
    } else {
        std::future::pending::<Option<AgentTaskStatus>>().await
    }
};
tokio::select! {
    Some(status) = status_changed => {
        update(&mut state, Event::AgentStatus(status));
    }
    // ...
}
```

### Heartbeat message format (automation/heartbeat.rs:320-327)
```rust
let msg = InboundMessage {
    id: format!("heartbeat-{}", now.timestamp()),
    channel: "heartbeat".to_string(),
    chat_id: self.chat_id.clone(),  // from config.heartbeat.report_to
    user_id: "system".to_string(),
    text: Some(format!("HEARTBEAT — You are running autonomously. ...")),
    // ...
};
```

### Heartbeat chat_id (main.rs:2293-2298)
```rust
let heartbeat_chat_id = config.heartbeat.report_to.clone()
    .unwrap_or_else(|| "heartbeat".to_string());
```
If `report_to` = user's Telegram chat ID, heartbeat shares the same worker slot.

---

## 4. Design: Mission Control

### Core Principle

**Defer routing until classified.** Today, ALL messages during busy go to pending queue unconditionally. Mission Control removes this unconditional push and lets the interceptor LLM classify first, THEN route:

| Classification | Token | Routing |
|---------------|-------|---------|
| Amendment to current task | `[AMEND]` | Push to pending queue (injected into tool results) |
| New independent order | `[QUEUE]` | Push to order queue (processed after current task) |
| Cancel request | `[CANCEL]` | Interrupt + cancel token |
| Status query or casual chat | `[CHAT]` | Message consumed by interceptor response |

The 1-3s delay for `[AMEND]` classification is acceptable — tool rounds take several seconds each, and the agent checks pending at each tool execution boundary.

### Architecture

```
User msg arrives while is_busy == true
  │
  ├─ /status → Fast-path: instant report (NO LLM call)
  ├─ /queue  → Fast-path: show queued orders (NO LLM call)
  ├─ /stop   → Fast-path: cancel (existing behavior)
  │
  └─ Free-form text → Mission Control LLM classifies:
       │
       ├─ [AMEND]  → pending queue → injected into current task's tool results
       ├─ [QUEUE]  → order queue → dispatched after current task completes
       ├─ [CANCEL] → interrupt + cancel_token.cancel()
       └─ [CHAT]   → interceptor responds, message consumed
```

### ChatSlot Evolution

```rust
struct ChatSlot {
    tx: tokio::sync::mpsc::Sender<InboundMessage>,
    interrupt: Arc<AtomicBool>,
    is_heartbeat: Arc<AtomicBool>,
    is_busy: Arc<AtomicBool>,
    current_task: Arc<std::sync::Mutex<String>>,           // KEEP (backward compat)
    cancel_token: tokio_util::sync::CancellationToken,     // parent token (per-slot)
    // ── Mission Control additions ──
    status_tx: tokio::sync::watch::Sender<AgentTaskStatus>,  // NEW: per-slot, not per-message
    order_queue: OrderQueue,                                   // NEW: queued orders
    active_cancel: Arc<std::sync::Mutex<CancellationToken>>,  // NEW: per-task child token
}

struct QueuedOrder {
    original_msg: InboundMessage,  // full message for exact reconstruction
    queued_at: std::time::Instant,
}

type OrderQueue = Arc<std::sync::Mutex<std::collections::VecDeque<QueuedOrder>>>;
```

### Status Watch Lifecycle

**Before (per-message, dropped):**
```
worker loop iteration:
  create (status_tx, _status_rx) ← receiver dropped immediately
  process_message(status_tx) ← agent emits to /dev/null
```

**After (per-slot, persistent):**
```
slot creation:
  create (status_tx, _) ← sender stored on ChatSlot

worker loop iteration:
  status_tx.send_modify(|s| *s = AgentTaskStatus::default()) ← reset per task
  process_message(status_tx.clone()) ← agent emits, interceptor can read

interceptor:
  status_tx.borrow() ← read current phase (zero-cost, no allocation)
```

`watch::Sender::borrow()` returns `Ref<AgentTaskStatus>` via `&self` — no mutex, no receiver needed.

### Per-Task CancellationToken

**Bug:** `cancel_token` is created once per slot. Once `cancel()` is called, it stays cancelled forever. Next task inherits a dead token.

**Fix:** Use `child_token()` pattern:
```rust
// At task start (before process_message):
let task_cancel = cancel_token_clone.child_token();
*active_cancel_clone.lock().unwrap() = task_cancel.clone();
// Pass task_cancel to process_message

// Interceptor / /stop reads active_cancel:
if let Ok(ct) = active_cancel.lock() { ct.cancel(); }
```

Parent token persists per-slot. Each task gets a fresh child. Cancelling parent cancels all children (graceful shutdown). Cancelling child only affects that task.

### Interceptor Prompt (Mission Control)

```
=== MISSION CONTROL ===
You are Tem's MISSION CONTROL. Your main self is busy working.

FOREGROUND TASK:
  Phase: {phase_display}
  Elapsed: {elapsed}s | Rounds: {rounds} | Tools run: {tools} | Cost: ${cost}

BACKGROUND (Perpetuum):
  {perpetuum_temporal_string_or_"None active"}

QUEUED ORDERS: {count}

The user says: "{user_message}"

Classify and respond (1-3 sentences max). End with EXACTLY ONE token:
[AMEND] — user is correcting/adding to the CURRENT task
[QUEUE] — user wants something NEW done AFTER the current task
[CANCEL] — user wants to STOP the current task
[CHAT] — user is chatting or asking about status

Rules:
- For status questions: describe what you're doing using the phase info, then end with [CHAT]
- For [QUEUE]: confirm the order is queued
- For [AMEND]: acknowledge the update
- NEVER use [CANCEL] unless user clearly wants to stop
=== END MISSION CONTROL ===
```

### Fast-Path Commands (no LLM call)

**`/status`** — reads `status_tx.borrow()` + `perpetuum_temporal.read().await` + `order_queue.lock()`:
```
Active task: Running shell_exec (1/3, round 2)
Rounds: 2 | Tools run: 3 | 12s elapsed | $0.0023

Background:
  Monitor "AWS billing" — next check in 4m
  Alarm "Meeting" — fires at 15:00

Queued orders: 1
```

**`/queue`** — reads `order_queue.lock()`:
```
Queued orders:
  1. "Check my email" (queued 30s ago)
  2. "Summarize today's news" (queued 15s ago)
```

### Order Queue Drain

After task completion, AFTER the existing pending drain (main.rs:4765-4793):

```rust
// Pop next queued order and dispatch to worker
if let Ok(mut oq) = order_queue_worker.lock() {
    if let Some(next_order) = oq.pop_front() {
        // next_order.original_msg is a full InboundMessage
        self_tx.try_send(next_order.original_msg);
    }
}
```

Pending drain runs first (existing behavior, re-queues unconsumed amendments). Then order queue pops the next new order. Worker loop picks it up via `chat_rx.recv()`.

### LLM Failure Fallback

On interceptor LLM call failure: push to pending (conservative = treat as amendment) + send hardcoded ack:
```
"Got your message — I'll look at it when I finish what I'm working on."
```

### Heartbeat Fix

Two changes:

1. **Don't set `current_task` for heartbeats** (main.rs:4190):
```rust
if !is_hb {
    if let Ok(mut ct) = current_task_clone.lock() { *ct = msg.text.as_deref()...; }
}
```

2. **Skip interceptor for heartbeat tasks** (main.rs:2455):
```rust
if slot.is_busy.load(Ordering::Relaxed)
    && !slot.is_heartbeat.load(Ordering::Relaxed)
{
    // Mission Control fires
}
```

When heartbeat is busy and user messages arrives: heartbeat gets preempted (existing lines 2426-2432 set interrupt + cancel), message falls through to worker channel naturally after heartbeat exits.

---

## 5. Race Condition Analysis

### Race 1: Interceptor fires after task completes

**Scenario:** Task finishes, `is_busy` → false, but interceptor's LLM call is still in flight.

**Impact:** Interceptor returns `[CANCEL]` and cancels a dead token. Or returns `[QUEUE]` and pushes to order queue.

**Mitigation:** `active_cancel` (per-task child token) is already dead when the task finishes. Cancelling a dead child is a no-op. `[QUEUE]` pushing to order queue is safe — the order gets dispatched immediately when the worker loops back (drain happens before idle).

### Race 2: Multiple interceptors for same chat

**Scenario:** User sends msg1 and msg2 rapidly while agent is busy. Two interceptor LLM calls fire concurrently.

**Impact:** Both classify independently. Both may push to pending or order queue.

**Mitigation:** Both pending queue and order queue are behind `std::sync::Mutex`. Pushes are atomic. Order is preserved (msg1 interceptor likely finishes before msg2's). No data corruption.

### Race 3: Heartbeat preemption timing

**Scenario:** Heartbeat is busy, user sends message. Heartbeat is preempted (interrupt + cancel), but `is_busy` may still be true.

**Impact:** With the heartbeat fix (`!slot.is_heartbeat.load()`), interceptor is skipped. Message falls through to worker channel. But worker is still processing the heartbeat.

**Mitigation:** The message sits in `chat_tx` (capacity 4) until the heartbeat worker exits its loop iteration. Worker picks it up on next `chat_rx.recv()`. Heartbeat cancellation causes the agent to return early (interrupt flag checked). Minimal delay.

### Race 4: Pending consumed before interceptor classifies

**Scenario (eliminated by design):** In the old design, pending push was unconditional. Agent might consume it before interceptor classifies. With Mission Control, pending push only happens AFTER classification returns `[AMEND]`. No race.

### Race 5: Order queue drain during panic

**Scenario:** Worker panics during task processing. Outer catch_unwind fires (main.rs:4808).

**Impact:** Panic handler should NOT clear order queue — queued orders are still valid work.

**Mitigation:** Panic handler (main.rs:4830-4831) only clears `is_busy`, `is_heartbeat`, `interrupt`. Order queue is untouched. Worker loop continues, picks up next message.

---

## 6. Perpetuum Harmony

### Current Perpetuum architecture

Perpetuum manages 5 concern types (alarms, monitors, recurring, initiatives, self-work) via a time-based scheduler (Pulse). Up to 20 concurrent dispatches. Notifications go directly through `channel_map` (bypass `msg_tx`). Agent interacts with Perpetuum via tools (`create_alarm`, `create_monitor`, `list_concerns`, etc.).

### How Mission Control harmonizes

| Perpetuum aspect | How Mission Control handles it |
|-----------------|-------------------------------|
| Background concerns (monitors, alarms) | Visible via `perpetuum_temporal` in interceptor prompt and `/status` |
| Heartbeat interference | Fixed: skip interceptor for heartbeat tasks, don't poison `current_task` |
| Multiple task types (long, short, scheduled) | Order queue handles sequential dispatch; Perpetuum handles concurrent background work |
| User wants to manage concerns mid-task | `[QUEUE]` the Perpetuum command → agent processes it after current task using Perpetuum tools |
| Temporal context | `perpetuum_temporal.read().await` provides cached active concerns list (zero DB cost) |

### Task model alignment

```
FOREGROUND (sequential, per-chat):
  Worker processes one task at a time
  Order queue provides sequential new-order handling
  Pending queue provides mid-task amendments

BACKGROUND (concurrent, Perpetuum):
  Up to 20 concern dispatches in parallel
  Monitors, alarms, recurring tasks run independently
  Notifications go directly to user via channel_map
  Visible to user via /status and interceptor prompt
```

This matches Perpetuum's design: the agent handles foreground conversation (sequential, context-dependent), while Perpetuum handles background autonomous work (concurrent, independent).

---

## 7. Implementation Phases

### Phase A: Heartbeat fix (zero dependencies)
- Guard `current_task` assignment with `!is_hb` (main.rs:4190)
- Guard interceptor with `!slot.is_heartbeat.load()` (main.rs:2455)
- **Risk: ZERO** — purely defensive, no behavior change for non-heartbeat paths

### Phase B: Per-task CancellationToken (zero dependencies)
- Add `active_cancel: Arc<Mutex<CancellationToken>>` to ChatSlot
- Create child token per task, store in `active_cancel`
- Interceptor and `/stop` read from `active_cancel`
- **Risk: LOW** — changes token lifecycle but preserves cancel semantics

### Phase C: Display impl for AgentTaskPhase (zero dependencies)
- Add `impl std::fmt::Display for AgentTaskPhase` in agent_task_status.rs
- Human-readable: "Thinking (round 2)", "Running shell_exec (1/3, round 2)", etc.
- **Risk: ZERO** — purely additive

### Phase D: ChatSlot evolution (depends on B, C)
- Add `status_tx`, `order_queue`, `active_cancel` to ChatSlot
- Move `status_tx` from per-message to per-slot creation
- Reset status at task start via `send_modify`
- Pass clones to worker loop
- **Risk: LOW** — structural change but no behavior change yet

### Phase E: Fast-path commands (depends on D)
- Add `/status` and `/queue` handling in dispatch loop (after `/stop`, before `is_busy` check)
- Format using `AgentTaskPhase::Display` + perpetuum_temporal + order_queue
- **Risk: ZERO** — new commands, no existing behavior changed

### Phase F: Mission Control interceptor (depends on D, E)
- Remove unconditional pending push (lines 2460-2465)
- Replace interceptor block with Mission Control prompt and routing
- Clone additional context (status_tx, order_queue, perpetuum_temporal, etc.)
- Parse classification tokens, route accordingly
- Add LLM failure fallback
- **Risk: MEDIUM** — changes routing behavior for mid-task messages

### Phase G: Order queue drain (depends on D, F)
- Add order queue pop after pending drain (main.rs:4793)
- Dispatch via `self_tx.try_send()`
- Ensure panic handler preserves order queue
- **Risk: LOW** — additive, runs after existing drain

### Recommended order: A → B → C → D → E → F → G

Each phase is independently committable and testable.

---

## 8. Files Modified

| File | Changes | Phase |
|------|---------|-------|
| `src/main.rs` | ChatSlot evolution, heartbeat fix, fast-path commands, Mission Control interceptor, order queue drain, per-task CancellationToken | A-G |
| `crates/temm1e-agent/src/agent_task_status.rs` | Add `Display` impl for `AgentTaskPhase` | C |

No new files. No new crates. No dependency changes.

---

## 9. Verification Plan

### Compilation gates
```bash
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
```

### Manual tests via CLI chat

1. **Observability test:** Start a long task (multi-round tool use), send `/status` mid-task → should see real-time phase info
2. **Queue test:** Start a long task, send "also check X after" → should get `[QUEUE]` ack, then X processed after task completes
3. **Amend test:** Start "write a Python script", send "use pathlib" → should be injected into tool results (agent acknowledges mid-task)
4. **Cancel test:** Start a task, send "stop that" → interceptor classifies `[CANCEL]`, task cancelled
5. **Heartbeat test:** Enable heartbeat, verify `/status` shows user task phase (not heartbeat text)
6. **Multiple orders test:** Queue 3 orders, verify they process sequentially after current task
7. **Perpetuum test:** Create a monitor, then during a user task send `/status` → should show both foreground task and background monitor

### Regression tests

- All 1312+ existing tests must pass
- No behavior change for idle-worker messages (is_busy == false path untouched)
- `/stop` still works (fast-path, unchanged)
- Hive mini-agents unaffected (pass None for all Mission Control params)

---

## 10. Open Questions

1. **Should `/status` and `/queue` work when NOT busy?** Current design: they're in the dispatch path before `is_busy` check, so they'd fire for idle workers too. This seems useful — user can check background Perpetuum concerns even when Tem is idle.

2. **Should the interceptor use a cheaper model?** Currently uses the same provider/model as the main agent. A smaller model (if available) would reduce cost and latency. Could be a follow-up optimization.

3. **Max order queue size?** Currently unbounded. Should we cap at, say, 10 orders? With a "queue full" response? Probably yes as a safety measure.

4. **Should queued orders show in Perpetuum's temporal context?** Currently no — order queue is separate from Perpetuum concerns. Could be unified in a future iteration.
