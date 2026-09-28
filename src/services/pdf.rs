use std::path::Path;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PdfMetrics {
    pub page_count: usize,
    pub content_density_pct: f32,
    pub is_sparse: bool,
}

/// Reads a PDF and returns the total page count.
pub fn get_page_count(pdf_path: &Path) -> Result<usize, anyhow::Error> {
    let doc = lopdf::Document::load(pdf_path)?;
    Ok(doc.get_pages().len())
}

/// Estimates the content density percentage of page 1 of a PDF (0.0 to 100.0%).
/// Section 5: under 60.0% density is classified as `ready_sparse`.
pub fn estimate_content_density(pdf_path: &Path) -> Result<f32, anyhow::Error> {
    let doc = lopdf::Document::load(pdf_path)?;
    let pages = doc.get_pages();
    if pages.is_empty() {
        return Ok(0.0);
    }

    let (&_first_page_num, &first_page_id) = pages.iter().next().unwrap();
    let content_bytes = doc.get_page_content(first_page_id);
    let content_str = String::from_utf8_lossy(&content_bytes);

    let re_bt = regex::Regex::new(r"(?s)BT\s+(.*?)\s+ET")?;
    let re_td = regex::Regex::new(r"(-?\d+(?:\.\d+)?)\s+(-?\d+(?:\.\d+)?)\s+T[dD]")?;

    let mut lowest_y = 0.0f64;
    let mut highest_y = 0.0f64;
    let mut found_blocks = false;

    for block in re_bt.captures_iter(&content_str) {
        found_blocks = true;
        let block_str = &block[1];
        let mut block_y = 0.0f64;
        for cap in re_td.captures_iter(block_str) {
            if let Ok(y) = cap[2].parse::<f64>() {
                block_y += y;
                if block_y < lowest_y {
                    lowest_y = block_y;
                }
                if block_y > highest_y {
                    highest_y = block_y;
                }
            }
        }
    }

    let vertical_span = highest_y - lowest_y;
    if !found_blocks || vertical_span <= 0.0 {
        // Fallback: estimate based on word count (350 words ~= 80% full page)
        if let Ok(text) = doc.extract_text(&[1]) {
            let words = text.split_whitespace().count() as f32;
            let density = (words / 450.0 * 100.0).clamp(0.0, 100.0);
            return Ok(density);
        }
        return Ok(0.0);
    }

    // Standard letter printable height is 650.0 pt (792 pt height - 2*0.6in margins)
    let printable_height = 650.0f64;
    let density_pct = ((vertical_span / printable_height) * 100.0).clamp(0.0, 100.0) as f32;
    Ok(density_pct)
}

/// Inspects page count and content density in a single call.
pub fn inspect_pdf(pdf_path: &Path) -> Result<PdfMetrics, anyhow::Error> {
    let page_count = get_page_count(pdf_path)?;
    let content_density_pct = if page_count == 1 {
        estimate_content_density(pdf_path)?
    } else {
        100.0
    };

    let is_sparse = page_count == 1 && content_density_pct < 60.0;

    Ok(PdfMetrics {
        page_count,
        content_density_pct,
        is_sparse,
    })
}
