// frontend/js/labels.js — Human-language mappings for ResumeForge
// Replaces internal technical jargon with plain, human-readable labels.

const LABELS = {
  // Five pipeline stages (Section 4)
  nodes: {
    jd_analysis: {
      id: "jd_analysis",
      title: "Reading the job description",
      shortTitle: "Job Description",
      description: "Extracting the target role, core technical skills, and key qualifications from the job posting.",
      idleStatus: "Waiting to analyze requirements",
      activeStatus: "Extracting role, core skills, and qualifications...",
      completeStatus: "Requirements analyzed successfully"
    },
    project_retrieval: {
      id: "project_retrieval",
      title: "Finding your best projects",
      shortTitle: "Project Evidence",
      description: "Scanning your indexed GitHub repositories and ranking by technical relevance to this job.",
      idleStatus: "Waiting to rank projects",
      activeStatus: "Ranking repositories by technical relevance...",
      completeStatus: "Matching projects selected"
    },
    content_generation: {
      id: "content_generation",
      title: "Writing your resume",
      shortTitle: "Drafting",
      description: "Drafting tailored professional summary, work history, and project bullets grounded in evidence.",
      idleStatus: "Waiting to generate draft",
      activeStatus: "Writing evidence-grounded resume content...",
      completeStatus: "Draft generated successfully"
    },
    evidence_validation: {
      id: "evidence_validation",
      title: "Checking every claim is true",
      shortTitle: "Fact Check",
      description: "Verifying that every metric, number, and technology is backed by verified code evidence or master history.",
      idleStatus: "Waiting to verify claims",
      activeStatus: "Checking all metrics and facts against evidence...",
      completeStatus: "All claims verified"
    },
    render_compile: {
      id: "render_compile",
      title: "Fitting it to one page",
      shortTitle: "Page Fit",
      description: "Compiling LaTeX typography and dynamically adjusting layout to fit cleanly on exactly one page.",
      idleStatus: "Waiting to compile",
      activeStatus: "Compiling and compacting to one page...",
      completeStatus: "Formatted to single page"
    }
  },

  // Event types to human descriptions
  eventTypes: {
    command_started: "Stage started",
    node_started: "Stage started",
    step_update: "Progress update",
    command_output: "Agent output",
    artifact_produced: "Artifact produced",
    node_warning: "Correction noted",
    node_done: "Stage completed",
    node_error: "Stage encountered an issue",
    result: "Final resume ready"
  },

  // High-level resume statuses
  resumeStatuses: {
    queued: { label: "Queued", badgeClass: "status-queued" },
    analysing_job: { label: "Reading job description", badgeClass: "status-running" },
    analyzing_jd: { label: "Reading job description", badgeClass: "status-running" },
    retrieving_projects: { label: "Finding best projects", badgeClass: "status-running" },
    generating_content: { label: "Writing resume", badgeClass: "status-running" },
    validating_evidence: { label: "Checking facts", badgeClass: "status-running" },
    rendering_latex: { label: "Fitting to one page", badgeClass: "status-running" },
    compiling_pdf: { label: "Compiling PDF", badgeClass: "status-running" },
    checking_pages: { label: "Checking page count", badgeClass: "status-running" },
    ready: { label: "Ready", badgeClass: "status-success" },
    ready_sparse: { label: "Ready (Sparse)", badgeClass: "status-success" },
    completed: { label: "Ready", badgeClass: "status-success" },
    failed: { label: "Stopped", badgeClass: "status-error" },
    cancelled: { label: "Cancelled", badgeClass: "status-neutral" },
    ai_timeout: { label: "Timed out", badgeClass: "status-error" },
    ai_empty_response: { label: "Empty AI response", badgeClass: "status-error" },
    ai_hung: { label: "AI unresponsive", badgeClass: "status-error" },
    ai_invalid_json: { label: "Formatting error", badgeClass: "status-error" },
    ai_tool_denied: { label: "Security blocked", badgeClass: "status-error" },
    ai_error: { label: "AI error", badgeClass: "status-error" },
    validation_error: { label: "Fact check issue", badgeClass: "status-error" },
    latex_error: { label: "Typesetting issue", badgeClass: "status-error" },
    page_limit_error: { label: "Page limit exceeded", badgeClass: "status-error" },
    github_error: { label: "Repository issue", badgeClass: "status-error" }
  },

  // Error mappings: what happened, why, exact next step (Section 5)
  errors: {
    ai_empty_response: {
      what: "The AI agent finished without returning content",
      why: "The Antigravity process completed without emitting response text.",
      next: "Click 'Regenerate' to run the generation step again."
    },
    ai_timeout: {
      what: "The writing process took too long",
      why: "The Antigravity response exceeded the allowed time limit (30s).",
      next: "Check your internet connection, then click 'Regenerate'."
    },
    ai_hung: {
      what: "The AI process stopped responding",
      why: "The background agent process became inactive and was safely terminated.",
      next: "Click 'Regenerate' to launch a clean generation turn."
    },
    ai_invalid_json: {
      what: "The draft formatting could not be read",
      why: "The AI output could not be parsed into the required resume schema.",
      next: "Click 'Regenerate' to produce a clean draft."
    },
    validation_error: {
      what: "Resume facts could not be verified",
      why: "Generated claims lacked backing evidence in your GitHub repositories or master resume.",
      next: "Make sure your master resume in Settings contains your work history, then try again."
    },
    latex_error: {
      what: "Document layout compilation error",
      why: "The Tectonic typesetting engine encountered an unexpected formatting error.",
      next: "Check your master resume template in Settings to make sure it compiles cleanly."
    },
    page_limit_error: {
      what: "Could not fit onto one page",
      why: "Even after dropping low-priority bullets and compacting margins, content exceeded one page.",
      next: "Shorten your master resume descriptions or add focus instructions, then regenerate."
    },
    default_error: {
      what: "Resume generation encountered an issue",
      why: "An unexpected condition occurred during processing.",
      next: "Click 'Regenerate' or review the activity log for details."
    }
  }
};

// Helper: Get node definition
function getNodeInfo(nodeKey) {
  return LABELS.nodes[nodeKey] || {
    id: nodeKey,
    title: nodeKey,
    shortTitle: nodeKey,
    description: "Processing pipeline stage.",
    idleStatus: "Waiting",
    activeStatus: "Working...",
    completeStatus: "Done"
  };
}

// Helper: Format event type
function formatEventName(eventType) {
  return LABELS.eventTypes[eventType] || eventType.replace(/_/g, ' ');
}

// Helper: Format resume status
function formatResumeStatus(status) {
  return LABELS.resumeStatuses[status] || { label: status || "Unknown", badgeClass: "status-neutral" };
}

// Helper: Get friendly error explanation
function formatErrorExplanation(errorStage, errorDetail) {
  const errKey = (errorStage || "").toLowerCase();
  for (const [key, val] of Object.entries(LABELS.errors)) {
    if (errKey.includes(key) || (errorDetail && errorDetail.toLowerCase().includes(key))) {
      return val;
    }
  }
  return {
    what: "Resume generation stopped",
    why: errorDetail || "A processing step could not be completed.",
    next: "Click 'Regenerate' to run the generation again, or check Settings."
  };
}

// Helper: Clean plain-language evidence explanation
function formatEvidenceItem(item) {
  if (typeof item === 'string') return item;
  if (item.reason) {
    if (item.section === 'skills') {
      return `Filtered skill: '${item.item}' was removed because it was not found in your master resume or project evidence.`;
    }
    if (item.section === 'summary') {
      return `Summary adjusted: '${item.item}' was removed because it lacked backing evidence in your master facts.`;
    }
    return `Removed claim: '${item.item}' (${item.reason}).`;
  }
  if (item.claim) {
    return `Verified: '${item.claim}' grounded in project evidence.`;
  }
  return JSON.stringify(item);
}
