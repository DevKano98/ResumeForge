use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentGenerationInput {
    pub candidate_name: String,
    pub master_facts: MasterFacts,
    pub jd_analysis: JdAnalysis,
    pub ranked_projects: Vec<RankedProjectInput>,
    pub extra_instructions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct MasterFacts {
    pub candidate_name: String,
    pub contact: Option<String>,
    pub current_summary: Option<String>,
    pub known_skills: Vec<String>,
    pub education: Vec<EducationFact>,
    pub work_history: Vec<WorkHistoryFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct EducationFact {
    pub institution: String,
    pub degree: String,
    pub date_range: Option<String>,
    pub location: Option<String>,
    pub highlights: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct WorkHistoryFact {
    pub title: String,
    pub company: String,
    pub date_range: String,
    pub location: Option<String>,
    pub highlights: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JdAnalysis {
    pub target_company: Option<String>,
    pub target_role: String,
    pub core_skills: Vec<String>,
    pub secondary_skills: Vec<String>,
    pub key_requirements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedProjectInput {
    pub project_id: i64,
    pub name: String,
    pub summary: String,
    pub technologies: Vec<String>,
    pub evidence: Vec<ProjectEvidenceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEvidenceItem {
    pub id: i64,
    pub claim: String,
    pub source_file: String,
    pub line_start: Option<i64>,
    pub line_end: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedResumeContent {
    pub summary: String,
    pub skills: GeneratedSkills,
    pub experience: Vec<GeneratedExperienceItem>,
    pub projects: Vec<GeneratedProjectItem>,
    #[serde(default)]
    pub achievements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedSkills {
    pub languages: Vec<String>,
    pub frameworks_and_tools: Vec<String>,
    pub core_concepts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedExperienceItem {
    pub title: String,
    pub company: String,
    pub date_range: String,
    pub location: Option<String>,
    pub bullets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedProjectItem {
    pub project_id: i64,
    pub name: String,
    pub bullets: Vec<GeneratedProjectBullet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedProjectBullet {
    pub text: String,
    #[serde(default)]
    pub evidence_ids: Vec<i64>,
}
