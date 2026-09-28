use resumeforge::services::{latex, pdf};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Verification 1: Content Density Benchmark on Realistic Content ===");

    let temp_dir = PathBuf::from("data/temp/density_benchmark");
    tokio::fs::create_dir_all(&temp_dir).await?;

    let classic_src = tokio::fs::read_to_string("data/templates/starter/classic.tex").await?;
    let modern_src = tokio::fs::read_to_string("data/templates/starter/modern.tex").await?;

    // (a) Realistic well-filled 1-page resume content
    let full_content = r#"
\section{Professional Summary}
\ResumeSummary{
Staff Software Engineer with 8+ years architecting distributed streaming platforms, high-throughput financial transaction processing, and cloud-native infrastructure. Proven track record eliminating latency bottlenecks and leading engineering teams across resilient systems.
}

\section{Technical Skills}
\ResumeSkills{
\begin{itemize}[leftmargin=0.15in, label={}]
    \item \textbf{Languages:} Rust, Go, Python, C++, TypeScript, SQL, Bash
    \item \textbf{Frameworks \& Storage:} Tokio, Axum, gRPC, Kafka, PostgreSQL, SQLite, Redis, Docker, Kubernetes
    \item \textbf{Core Domains:} Distributed Consensus, High-Concurrency Networking, Zero-Copy I/O, Observability
\end{itemize}
}

\section{Experience}
\ResumeExperience{
\textbf{Staff Infrastructure Engineer} \hfill \textbf{Jan 2022 -- Present} \\
\textit{Platform Scale Systems} \hfill \textit{Remote}
\begin{itemize}[leftmargin=0.15in]
    \item Architected distributed event stream engine handling 120,000 requests/sec with p99 latency under 2ms.
    \item Decreased cloud compute spend by \$420,000 annually through lock-free ring buffer memory allocation redesign.
    \item Designed zero-downtime rolling upgrade orchestration across 8 Kubernetes clusters with automated canary rollout.
\end{itemize}

\vspace{4pt}
\textbf{Senior Systems Engineer} \hfill \textbf{Mar 2019 -- Dec 2021} \\
\textit{CoreData Networks} \hfill \textit{San Francisco, CA}
\begin{itemize}[leftmargin=0.15in]
    \item Implemented custom LSM-tree persistence layer in Rust achieving 2.1M IOPS with deterministic tail bounds.
    \item Built end-to-end distributed tracing harness capturing kernel context switches and CPU cache misses.
    \item Mentored 6 software engineers in systems programming, async Tokio idioms, and performance profiling.
\end{itemize}
}

\section{Projects}
\ResumeProjects{
\textbf{ResilientQueue Engine} $|$ \emph{Rust, Tokio, SQLite WAL} \hfill \href{https://github.com/candidate/resilient-queue}{github.com/candidate/resilient-queue}
\begin{itemize}[leftmargin=0.15in]
    \item Engineered an asynchronous embedded message queue engine utilizing WAL mode with crash-safe transaction logging.
    \item Implemented real-time telemetry streaming over WebSockets with custom Axum middleware.
\end{itemize}

\vspace{4pt}
\textbf{VectorSync Index} $|$ \emph{Rust, AVX-512, SIMD} \hfill \href{https://github.com/candidate/vectorsync}{github.com/candidate/vectorsync}
\begin{itemize}[leftmargin=0.15in]
    \item Developed SIMD-accelerated high-dimensional vector similarity index with sub-millisecond k-NN retrieval.
\end{itemize}
}
"#;

    // (b) Genuinely sparse 1-page resume content
    let sparse_content = r#"
\section{Summary}
\ResumeSummary{
Software Engineer with interest in cloud development.
}

\section{Experience}
\ResumeExperience{
\textbf{Junior Developer} \hfill \textbf{2024 -- Present} \\
\textit{Tech Startup} \hfill \textit{Remote}
\begin{itemize}[leftmargin=0.15in]
    \item Assisted with bug fixes on customer facing dashboard.
\end{itemize}
}
"#;

    let re_sections =
        regex::Regex::new(r"(?s)\\section\{Professional Summary\}.*?\\end\{document\}").unwrap();
    let re_sections_modern =
        regex::Regex::new(r"(?s)\\section\{Summary\}.*?\\end\{document\}").unwrap();

    let classic_full = re_sections
        .replace(&classic_src, format!("{}\n\\end{{document}}", full_content))
        .to_string();
    let classic_sparse = re_sections
        .replace(
            &classic_src,
            format!("{}\n\\end{{document}}", sparse_content),
        )
        .to_string();

    let modern_full = re_sections_modern
        .replace(&modern_src, format!("{}\n\\end{{document}}", full_content))
        .to_string();
    let modern_sparse = re_sections_modern
        .replace(
            &modern_src,
            format!("{}\n\\end{{document}}", sparse_content),
        )
        .to_string();

    let test_cases = [
        ("classic_full", classic_full, true),
        ("classic_sparse", classic_sparse, false),
        ("modern_full", modern_full, true),
        ("modern_sparse", modern_sparse, false),
    ];

    println!("\nCompiling and evaluating density for all 4 test cases...\n");

    for (name, tex, should_be_well_filled) in test_cases {
        let pdf_dest = temp_dir.join(format!("{}.pdf", name));
        let compile_res = latex::compile_latex_content(&tex, &temp_dir, Some(&pdf_dest)).await?;
        assert!(compile_res.success, "Compilation failed for {}", name);

        let metrics = pdf::inspect_pdf(&pdf_dest)?;
        let pages = metrics.page_count;
        let density = metrics.content_density_pct;
        let is_sparse = metrics.is_sparse;

        println!(
            "Document: {:<15} | Pages: {} | Density: {:>5.1}% | is_sparse: {}",
            name, pages, density, is_sparse
        );

        if should_be_well_filled {
            assert_eq!(pages, 1, "Full resume must fit on 1 page");
            assert!(
                density >= 60.0,
                "Full resume density ({:.1}%) should be >= 60%!",
                density
            );
            assert!(!is_sparse, "Full resume must NOT be marked sparse");
        } else {
            assert_eq!(pages, 1, "Sparse resume must be 1 page");
            assert!(
                density < 60.0,
                "Sparse resume density ({:.1}%) must be < 60%!",
                density
            );
            assert!(is_sparse, "Sparse resume MUST be marked sparse");
        }
    }

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    println!("\n*** VERIFICATION 1 PASSED: Density heuristic cleanly separates full (>= 60%) from sparse (< 60%) ***\n");
    Ok(())
}
