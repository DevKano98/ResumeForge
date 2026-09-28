use std::path::PathBuf;

fn run_tectonic(tex_path: &str, out_dir: &str) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new("tectonic");
    cmd.arg(tex_path).arg("--outdir").arg(out_dir);

    if let Ok(user) = std::env::var("USERPROFILE") {
        let p = format!(
            "{}\\AppData\\Local\\agy\\bin;{}\\w64devkit\\bin;{}",
            user,
            user,
            std::env::var("PATH").unwrap_or_default()
        );
        cmd.env("PATH", p);
    }

    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        anyhow::bail!("Tectonic failed:\nstdout: {}\nstderr: {}", stdout, stderr);
    }
    Ok(())
}

fn get_pdf_page_count(pdf_path: &str) -> anyhow::Result<usize> {
    let doc = lopdf::Document::load(pdf_path)?;
    Ok(doc.get_pages().len())
}

fn main() -> anyhow::Result<()> {
    println!("=== Testing Functional Reality of \\tighten Macro ===");

    let temp_dir = PathBuf::from("data/temp/tighten_test");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    std::fs::create_dir_all(&temp_dir)?;

    let base_item = r#"
\vspace{6pt}
\textbf{Distributed Systems Component} $|$ \emph{Rust, Tokio, gRPC} \hfill \href{https://github.com/example}{github.com/example}
\begin{itemize}[leftmargin=0.15in]
    \item Engineered high-throughput event processing subsystem processing over 45,000 messages per second.
    \item Implemented zero-copy buffer serialization pipeline reducing heap allocations by 40\%.
\end{itemize}
"#;

    for template_name in ["classic", "modern"] {
        println!("\n--- Testing template: {} ---", template_name);
        let src_path = format!("data/templates/starter/{}.tex", template_name);
        let original_src = std::fs::read_to_string(&src_path)?;

        // Find the boundary where uncompressed spills onto page 2
        let mut count = 1;
        let mut uncompressed_pages = 1;
        let mut doc_text = String::new();

        while uncompressed_pages == 1 && count <= 15 {
            let mut repeated_items = String::new();
            for i in 1..=count {
                repeated_items.push_str(&base_item.replace(
                    "Distributed Systems Component",
                    &format!("Project Component \\#{}", i),
                ));
            }

            let candidate = original_src.replace(
                "\\end{document}",
                &format!(
                    "\\section{{Additional Projects}}\n\\ResumeProjects{{\n{}}}\n\\end{{document}}",
                    repeated_items
                ),
            );

            let test_tex = temp_dir.join(format!("{}_probe_{}.tex", template_name, count));
            std::fs::write(&test_tex, &candidate)?;
            run_tectonic(test_tex.to_str().unwrap(), temp_dir.to_str().unwrap())?;

            let test_pdf = temp_dir.join(format!("{}_probe_{}.pdf", template_name, count));
            uncompressed_pages = get_pdf_page_count(test_pdf.to_str().unwrap())?;
            println!(
                "  [Probe] {} components: {} page(s)",
                count, uncompressed_pages
            );

            if uncompressed_pages == 2 {
                doc_text = candidate;
                break;
            }
            count += 1;
        }

        assert_eq!(
            uncompressed_pages, 2,
            "Failed to construct a 2-page document"
        );
        println!(
            "  ✓ Document with {} components overflows to exactly 2 pages without \\tighten",
            count
        );

        // Now compile that exact same document with \tighten invoked
        let tightened_text = doc_text.replace("\\begin{document}", "\\begin{document}\n\\tighten");
        let tightened_tex = temp_dir.join(format!("{}_tightened.tex", template_name));
        std::fs::write(&tightened_tex, &tightened_text)?;
        run_tectonic(tightened_tex.to_str().unwrap(), temp_dir.to_str().unwrap())?;

        let tightened_pdf = temp_dir.join(format!("{}_tightened.pdf", template_name));
        let tightened_pages = get_pdf_page_count(tightened_pdf.to_str().unwrap())?;
        println!("  - With \\tighten applied: {} page(s)", tightened_pages);

        assert_eq!(
            tightened_pages, 1,
            "Tightened document MUST compress back down to 1 page!"
        );
        println!(
            "  ✓ SUCCESS: \\tighten compressed {} from 2 pages down to 1 page!",
            template_name
        );
    }

    println!("\n*** VERIFICATION CONFIRMED: \\tighten is functionally real and effective across all starter templates ***\n");
    Ok(())
}
