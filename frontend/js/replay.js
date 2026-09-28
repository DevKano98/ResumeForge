// frontend/js/replay.js — Replay demo engine for ResumeForge
// Replays a saved run sequence on the n8n canvas at adjustable speeds without calling agy.

const DEMO_RUN = {
  resume: {
    id: 999,
    company: "Cloudflare",
    role: "Senior Systems Engineer",
    status: "completed",
    page_count: 1,
    created_at: new Date().toISOString()
  },
  evidence: {
    accepted: [
      { section: "projects", claim: "Engineered distributed Raft consensus engine in Rust handling 50k ops/sec", project: "raft-consensus" },
      { section: "projects", claim: "Implemented zero-allocation TCP stream parser with custom ring buffer", project: "p2p-mesh" },
      { section: "experience", claim: "Maintained 99.99% service uptime across multi-region Kubernetes clusters", company: "Acme Cloud" },
      { section: "skills", claim: "Verified technical proficiencies: Rust, Go, Distributed Systems, Linux, Kubernetes", source: "master" }
    ],
    rejected: [
      { section: "projects", item: "reduced network latency by 72% across 10 regions", reason: "Could not be verified in repository commits or benchmarks" },
      { section: "skills", item: "Kubernetes Operator SDK", reason: "Not found in your master resume or cited project evidence" }
    ],
    warnings: []
  },
  events: [
    // 1. Reading the job description
    {
      resume_id: 999, seq: 1, ts: "2026-09-28T09:00:00Z", node: "jd_analysis", type: "command_started",
      payload: { message: "Reading job posting requirements..." }
    },
    {
      resume_id: 999, seq: 2, ts: "2026-09-28T09:00:02Z", node: "jd_analysis", type: "step_update",
      payload: { delta: "Found role: Senior Systems Engineer at Cloudflare" }
    },
    {
      resume_id: 999, seq: 3, ts: "2026-09-28T09:00:03Z", node: "jd_analysis", type: "node_done",
      payload: {
        role: "Senior Systems Engineer",
        company: "Cloudflare",
        core_skills: ["Rust", "Distributed Systems", "Networking", "Linux", "Go"]
      }
    },

    // 2. Finding your best projects
    {
      resume_id: 999, seq: 4, ts: "2026-09-28T09:00:04Z", node: "project_retrieval", type: "command_started",
      payload: { message: "Evaluating repository evidence against core skills..." }
    },
    {
      resume_id: 999, seq: 5, ts: "2026-09-28T09:00:05Z", node: "project_retrieval", type: "step_update",
      payload: { delta: "Scored 6 indexed repositories. Ranking top matches..." }
    },
    {
      resume_id: 999, seq: 6, ts: "2026-09-28T09:00:06Z", node: "project_retrieval", type: "artifact_produced",
      payload: {
        ranked_projects: [
          { name: "raft-consensus", score: 0.94, technologies: ["Rust", "Raft", "Distributed Systems"] },
          { name: "p2p-mesh", score: 0.88, technologies: ["Go", "Networking", "TCP"] },
          { name: "resumeforge", score: 0.82, technologies: ["Rust", "Axum", "SQLite"] }
        ]
      }
    },
    {
      resume_id: 999, seq: 7, ts: "2026-09-28T09:00:07Z", node: "project_retrieval", type: "node_done",
      payload: { count: 3, top: "raft-consensus" }
    },

    // 3. Writing your resume
    {
      resume_id: 999, seq: 8, ts: "2026-09-28T09:00:08Z", node: "content_generation", type: "command_started",
      payload: { message: "Antigravity AI generating tailored professional experience..." }
    },
    {
      resume_id: 999, seq: 9, ts: "2026-09-28T09:00:10Z", node: "content_generation", type: "command_output",
      payload: { chunk: "Drafting targeted summary and grounding achievements in Raft & P2P evidence..." }
    },
    {
      resume_id: 999, seq: 10, ts: "2026-09-28T09:00:12Z", node: "content_generation", type: "command_output",
      payload: { chunk: "Synthesizing project bullets with verified technical metrics..." }
    },
    {
      resume_id: 999, seq: 11, ts: "2026-09-28T09:00:14Z", node: "content_generation", type: "artifact_produced",
      payload: {
        summary: "Systems engineer specializing in high-throughput distributed systems, low-latency networking, and fault-tolerant consensus in Rust and Go.",
        experience_count: 2,
        project_count: 3
      }
    },
    {
      resume_id: 999, seq: 12, ts: "2026-09-28T09:00:15Z", node: "content_generation", type: "node_done",
      payload: { elapsed_ms: 7120 }
    },

    // 4. Checking every claim is true
    {
      resume_id: 999, seq: 13, ts: "2026-09-28T09:00:16Z", node: "evidence_validation", type: "command_started",
      payload: { message: "Running deterministic anti-hallucination guards..." }
    },
    {
      resume_id: 999, seq: 14, ts: "2026-09-28T09:00:17Z", node: "evidence_validation", type: "node_warning",
      payload: {
        warning: "Removed ungrounded metric: 'reduced network latency by 72% across 10 regions' was not found in commit benchmarks."
      }
    },
    {
      resume_id: 999, seq: 15, ts: "2026-09-28T09:00:18Z", node: "evidence_validation", type: "artifact_produced",
      payload: { accepted_count: 4, rejected_count: 1 }
    },
    {
      resume_id: 999, seq: 16, ts: "2026-09-28T09:00:19Z", node: "evidence_validation", type: "node_done",
      payload: { accepted: 4, rejected: 1 }
    },

    // 5. Fitting it to one page
    {
      resume_id: 999, seq: 17, ts: "2026-09-28T09:00:20Z", node: "render_compile", type: "command_started",
      payload: { message: "Compiling LaTeX typography with Tectonic..." }
    },
    {
      resume_id: 999, seq: 18, ts: "2026-09-28T09:00:21Z", node: "render_compile", type: "step_update",
      payload: { attempt: 1, max_attempts: 4, message: "Attempt 1 of 4: 2 pages detected (over limit). Dropping lowest relevance bullet..." }
    },
    {
      resume_id: 999, seq: 19, ts: "2026-09-28T09:00:22Z", node: "render_compile", type: "step_update",
      payload: { attempt: 2, max_attempts: 4, message: "Attempt 2 of 4: Compacting margins and line-height with \\tighten..." }
    },
    {
      resume_id: 999, seq: 20, ts: "2026-09-28T09:00:23Z", node: "render_compile", type: "node_done",
      payload: { page_count: 1, attempts_used: 2 }
    },
    {
      resume_id: 999, seq: 21, ts: "2026-09-28T09:00:24Z", node: "render_compile", type: "result",
      payload: { status: "completed", page_count: 1 }
    }
  ]
};

class ReplayController {
  constructor() {
    this.speed = 1.0;
    this.isPlaying = false;
    this.currentIndex = 0;
    this.timerId = null;
    this.onEventCallback = null;
    this.onCompleteCallback = null;
  }

  setSpeed(newSpeed) {
    this.speed = parseFloat(newSpeed) || 1.0;
  }

  start(onEvent, onComplete) {
    this.stop();
    this.onEventCallback = onEvent;
    this.onCompleteCallback = onComplete;
    this.currentIndex = 0;
    this.isPlaying = true;

    if (window.location.href.includes('state=done')) {
      for (const ev of DEMO_RUN.events) {
        if (this.onEventCallback) this.onEventCallback(ev);
      }
      this.isPlaying = false;
      if (this.onCompleteCallback) this.onCompleteCallback(DEMO_RUN);
      return;
    }

    if (window.location.href.includes('state=mid')) {
      for (let i = 0; i < 9; i++) {
        if (this.onEventCallback) this.onEventCallback(DEMO_RUN.events[i]);
      }
      this.currentIndex = 9;
      this.isPlaying = false;
      return;
    }

    this.scheduleNext();
  }

  scheduleNext() {
    if (!this.isPlaying) return;
    if (this.currentIndex >= DEMO_RUN.events.length) {
      this.isPlaying = false;
      if (this.onCompleteCallback) this.onCompleteCallback(DEMO_RUN);
      return;
    }

    const event = DEMO_RUN.events[this.currentIndex];
    this.currentIndex++;

    if (this.onEventCallback) {
      this.onEventCallback(event);
    }

    // Default delay between events: ~750ms / speed
    const baseDelay = 750;
    const delay = Math.max(100, Math.floor(baseDelay / this.speed));
    this.timerId = setTimeout(() => this.scheduleNext(), delay);
  }

  pause() {
    this.isPlaying = false;
    if (this.timerId) clearTimeout(this.timerId);
  }

  resume() {
    if (!this.isPlaying && this.currentIndex < DEMO_RUN.events.length) {
      this.isPlaying = true;
      this.scheduleNext();
    }
  }

  stop() {
    this.isPlaying = false;
    if (this.timerId) clearTimeout(this.timerId);
    this.currentIndex = 0;
  }
}

window.replayController = new ReplayController();
