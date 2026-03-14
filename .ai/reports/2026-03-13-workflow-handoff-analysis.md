# Workflow Handoff Analysis — Agent-to-Agent Communication Gaps

**Date:** 2026-03-13
**Context:** Develop-Review Autonomous workflow, schema-support team
**Observed during:** Task 1 and Task 2 of Schema Support implementation

## Problem Statement

The Develop-Review Autonomous workflow defines synchronous handoff
points between agents (e.g., Developer submits tests → Test Engineer
verifies → Developer implements). These handoffs rely on peer-to-peer
`SendMessage` calls between agents. In practice, messages between
agents silently fail to deliver or are not processed, causing stalls
that require lead intervention to resolve.

This undermines the "Autonomous" promise of the workflow — the lead
becomes a manual message broker instead of a coordinator.

## Observed Incidents

### Incident 1 — Task 1: Security Assessment Not Received by Test Engineer

- **What happened:** The Security Engineer delivered a detailed
  pre-implementation assessment with 10 required test scenarios.
  The Test Engineer never received it and explicitly asked the lead
  to forward the security test cases.
- **Lead action required:** Lead manually relayed all 10 security
  test scenarios to the Test Engineer.
- **Time lost:** ~2 minutes of idle wait before detection.

### Incident 2 — Task 2: Test Verification Request Not Received

- **What happened:** The Developer wrote all 65 tests and sent two
  messages to the Test Engineer requesting verification (at 06:15:40
  and 06:16:28). The Test Engineer remained idle and did not respond
  for over 15 minutes.
- **Evidence of misrouting:** The Developer's idle notification
  summaries show `[to test-engineer]` (hyphenated lowercase), while
  the agent's registered name is `Test Engineer` (with space). If
  `SendMessage` requires exact name matching, these messages were
  silently dropped.
- **Lead action required:** Lead sent a nudge message to the Test
  Engineer at 06:32, which woke them up. Test Engineer verified
  tests within 30 seconds of receiving the lead's message.
- **Time lost:** ~16 minutes of dead wait.

## Root Cause Analysis

### 1. Agent Name Mismatch

Agents guess each other's names when sending messages. The registered
names use spaces (`Test Engineer`, `Security Engineer`) but agents
sometimes use hyphenated forms (`test-engineer`, `security-engineer`).
If the messaging system requires exact name matching, messages are
silently dropped with no error feedback to the sender.

### 2. No Delivery Confirmation

`SendMessage` returns success when the message is queued, not when
it's delivered and processed. Senders have no way to know if their
message reached the recipient. The Developer sent two messages and
waited 16 minutes with no indication of failure.

### 3. No Timeout or Escalation Mechanism

The workflow defines handoff points but no timeouts. When a handoff
stalls, there is no automatic escalation. The lead must manually
notice the stall (by observing idle notifications without progress)
and intervene.

### 4. Idle State Ambiguity

An agent going idle means "turn ended, waiting for input." But from
the lead's perspective, idle after sending a message (normal) looks
identical to idle because a message was never received (stall). There
is no way to distinguish these states without manually checking.

## Impact

| Metric | Task 1 | Task 2 |
|--------|--------|--------|
| Stall duration | ~2 min | ~16 min |
| Lead interventions | 1 | 1 |
| Messages manually relayed | 1 (10 test scenarios) | 1 (nudge) |

In a 4-task workflow, if each task has 2-3 handoff points, the
expected number of stalls is significant. Each stall costs minutes
of wall-clock time and requires lead attention.

## Recommendations

### Short-Term (Apply to Remaining Tasks 3 & 4)

1. **Lead-relayed handoffs for critical transitions.** Instead of
   relying on Developer → Test Engineer direct messaging for the
   "tests written, please verify" handoff, the Developer reports
   to the lead, and the lead sends to the Test Engineer. This adds
   one message hop but eliminates the naming mismatch risk.

2. **Architect includes team roster in task messages.** When the
   Architect sends a task to the dev-team, include the exact
   registered names of all agents:
   ```
   Team roster:
   - Developer
   - Test Engineer
   - Security Engineer
   - Reviewer
   ```
   This gives agents the correct names to use in `SendMessage`.

3. **Lead monitors handoff points proactively.** At known handoff
   points (after Developer reports tests written, after Developer
   reports implementation complete), the lead should immediately
   relay rather than waiting to see if the peer-to-peer message
   lands.

### Medium-Term (Workflow Improvements)

4. **Require handoff acknowledgment.** Add to the workflow: the
   receiving agent must acknowledge receipt within 60 seconds. If
   no ack, the sender escalates to the Architect or lead.

5. **Standardize agent naming convention.** Use names without
   spaces across the blueprint (e.g., `test-engineer`,
   `security-engineer`). Update `.claude/agents/` definitions
   and all workflow documents.

6. **Add handoff timeout to workflow definition.** Define in the
   workflow that if a handoff is not acknowledged within N minutes,
   the lead intervenes automatically. This makes the monitoring
   obligation explicit rather than ad-hoc.

### Long-Term (Platform Improvements)

7. **Message delivery receipts.** `SendMessage` should return
   whether the message was delivered to the recipient, not just
   queued. Failed deliveries should return an error with the
   reason (e.g., "no agent with name 'test-engineer' found").

8. **Agent name discovery.** Agents should be able to query the
   team roster programmatically rather than guessing names from
   context.

## Decision

For the remaining Task 3 and Task 4 of this session, apply
short-term recommendations 1-3: lead-relayed handoffs at critical
transitions, and proactive monitoring at handoff points. Evaluate
medium-term recommendations for the next workflow revision.
