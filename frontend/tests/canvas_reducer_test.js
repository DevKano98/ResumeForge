// frontend/tests/canvas_reducer_test.js
// Unit tests for the pure canvasReducer function.
// Run with: node frontend/tests/canvas_reducer_test.js
// No browser, no DOM, no build step required.

"use strict";

// ---------------------------------------------------------------------------
// Minimal stubs for helpers canvasReducer calls
// ---------------------------------------------------------------------------

// Stub for getNodeInfo (normally loaded from labels.js)
function getNodeInfo(nodeKey) {
  const map = {
    jd_analysis:        { activeStatus: "Extracting role, core skills...", completeStatus: "Requirements analyzed" },
    project_retrieval:  { activeStatus: "Ranking repositories...",         completeStatus: "Matching projects selected" },
    content_generation: { activeStatus: "Writing resume content...",       completeStatus: "Draft generated" },
    evidence_validation:{ activeStatus: "Checking all facts...",           completeStatus: "All claims verified" },
    render_compile:     { activeStatus: "Compiling to one page...",        completeStatus: "Formatted to single page" },
  };
  return map[nodeKey] || { activeStatus: "Working...", completeStatus: "Done" };
}

// Stub for formatErrorExplanationWithDetail (normally loaded from labels.js)
function formatErrorExplanationWithDetail(stage, detail, extra) {
  if ((stage || "").includes("ai_timeout") && extra?.timeout_secs) {
    return {
      what: "The writing process took too long",
      why: `The Antigravity response exceeded the allowed time limit (${extra.timeout_secs}s).`,
      next: "Check your internet connection, then click 'Regenerate'."
    };
  }
  return { what: detail, why: detail, next: "Click 'Regenerate' to try again." };
}

// ---------------------------------------------------------------------------
// Load the reducer (CommonJS export)
// ---------------------------------------------------------------------------
// canvasReducer reads getNodeInfo + formatErrorExplanationWithDetail from its
// closure scope. We inject them by inlining the function text and eval-ing it.
// This is simpler than a full module bundler for a plain-JS project.

const fs = require("fs");
const src = fs.readFileSync(require("path").join(__dirname, "../js/canvas.js"), "utf8");

// Stub browser globals so Node doesn't throw on `window.canvasController = ...`
const windowStub = { canvasController: null };
const documentStub = {
  getElementById: () => null,
  createElementNS: () => ({ setAttribute: () => {}, appendChild: () => {}, addEventListener: () => {}, querySelectorAll: () => [] }),
};

// Extract canvasReducer body from the source (between the JSDoc and module.exports)
// We use a simple sandbox eval with the stubs in scope.
const sandboxFn = new Function(
  "getNodeInfo", "formatErrorExplanationWithDetail", "window", "document",
  src +
  "\nreturn canvasReducer;"
);
const canvasReducer = sandboxFn(getNodeInfo, formatErrorExplanationWithDetail, windowStub, documentStub);

// ---------------------------------------------------------------------------
// Tiny test harness
// ---------------------------------------------------------------------------
let passed = 0;
let failed = 0;

function assert(condition, description) {
  if (condition) {
    console.log(`  ✓  ${description}`);
    passed++;
  } else {
    console.error(`  ✗  FAIL: ${description}`);
    failed++;
  }
}

function test(label, fn) {
  console.log(`\n[TEST] ${label}`);
  fn();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function idleNode(key = "content_generation") {
  return {
    state: "idle",
    statusText: "Waiting",
    startTime: null,
    elapsed: 0,
    events: [],
    outputs: null,
    summary: null,
    attempt: 0,
    errorExplanation: null,
  };
}

function ev(type, node, payload = null) {
  return { type, node, payload, ts: new Date().toISOString() };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test("node_started → state becomes running", () => {
  const next = canvasReducer(idleNode(), ev("node_started", "content_generation"));
  assert(next.state === "running", "state === 'running'");
  assert(next.statusText === "Writing resume content...", "statusText uses label activeStatus");
  assert(next.startTime !== null, "startTime is set");
});

test("command_started also transitions to running", () => {
  const next = canvasReducer(idleNode("jd_analysis"), ev("command_started", "jd_analysis"));
  assert(next.state === "running", "state === 'running'");
  assert(next.statusText.includes("Extracting"), "statusText is the JD analysis activeStatus");
});

test("step_update message overrides statusText", () => {
  const node = Object.assign(idleNode(), { state: "running" });
  const next = canvasReducer(node, ev("step_update", "content_generation", { message: "Writing bullet 3 of 6" }));
  assert(next.statusText === "Writing bullet 3 of 6", "statusText updated from message");
});

test("step_update delta overrides statusText when no message", () => {
  const node = Object.assign(idleNode(), { state: "running" });
  const next = canvasReducer(node, ev("step_update", "content_generation", { delta: "streaming..." }));
  assert(next.statusText === "streaming...", "statusText updated from delta");
});

test("step_update attempt is recorded", () => {
  const node = Object.assign(idleNode("render_compile"), { state: "running" });
  const next = canvasReducer(node, ev("step_update", "render_compile", { attempt: 3 }));
  assert(next.attempt === 3, "attempt === 3");
});

test("artifact_produced sets outputs and summary for ranked_projects", () => {
  const payload = { ranked_projects: ["repo-a", "repo-b", "repo-c"] };
  const next = canvasReducer(idleNode("project_retrieval"), ev("artifact_produced", "project_retrieval", payload));
  assert(next.outputs === payload, "outputs set to payload");
  assert(next.summary === "Selected 3 relevant repositories.", "summary mentions count");
});

test("node_done → state complete, statusText from completeStatus", () => {
  const next = canvasReducer(
    Object.assign(idleNode("evidence_validation"), { state: "running" }),
    ev("node_done", "evidence_validation", { accepted: 12, rejected: 2 })
  );
  assert(next.state === "complete", "state === 'complete'");
  assert(next.summary.includes("12 facts"), "summary mentions accepted count");
  assert(next.summary.includes("2 ungrounded"), "summary mentions rejected count");
});

test("node_done render_compile page_count → statusText shows attempt", () => {
  const next = canvasReducer(
    Object.assign(idleNode("render_compile"), { state: "running" }),
    ev("node_done", "render_compile", { page_count: 1, attempts_used: 3 })
  );
  assert(next.state === "complete", "state === 'complete'");
  assert(next.statusText.includes("3 attempts"), "statusText mentions attempts_used");
});

test("node_error → state error, uses backend detail, errorExplanation set", () => {
  const payload = { stage: "ai_error", detail: "internal server error" };
  const next = canvasReducer(idleNode(), ev("node_error", "content_generation", payload));
  assert(next.state === "error", "state === 'error'");
  assert(next.summary === "internal server error", "summary equals backend detail");
  assert(next.errorExplanation !== null, "errorExplanation is populated");
});

test("node_error ai_timeout with timeout_secs → why includes actual seconds", () => {
  const payload = { stage: "ai_timeout", detail: "timed out after 120 seconds", timeout_secs: 120 };
  const next = canvasReducer(idleNode(), ev("node_error", "content_generation", payload));
  assert(next.state === "error", "state === 'error'");
  assert(next.errorExplanation.why.includes("120s"), `why includes '120s' — got: ${next.errorExplanation.why}`);
});

test("reducer does NOT mutate the input nodeState", () => {
  const original = idleNode();
  const snap = JSON.stringify(original);
  canvasReducer(original, ev("node_started", "content_generation"));
  assert(JSON.stringify(original) === snap, "original nodeState is unchanged after reduce");
});

test("unknown event type returns unchanged clone", () => {
  const node = idleNode();
  const next = canvasReducer(node, ev("totally_unknown_event", "content_generation", {}));
  assert(next.state === "idle", "state unchanged");
  assert(next !== node, "returns a new object reference");
});

test("result event returns unchanged node (pipeline complete handled by caller)", () => {
  const node = idleNode();
  const next = canvasReducer(node, ev("result", "render_compile", { pdf_url: "/api/resumes/1/pdf" }));
  assert(next.state === "idle", "state unchanged for result event");
});

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------
console.log(`\n${"─".repeat(50)}`);
console.log(`canvas_reducer_test: ${passed} passed, ${failed} failed`);
if (failed > 0) {
  process.exit(1);
}
