use domain::entities::KnowledgeChunk;

pub const DEFAULT_MAX_CHUNK_CHARS: usize = 1_200;
pub const DEFAULT_OVERLAP_CHARS: usize = 150;

/// Split Markdown into context-preserving chunks:
/// 1. Tracks full heading breadcrumb stack (`H1 > H2 > H3 > H4`).
/// 2. Respects fenced code blocks (` ``` ` / `~~~`) so `# comment` inside code is not treated as a heading.
/// 3. Splits long sections into atomic Markdown blocks (paragraphs, code blocks, tables, bullet lists).
/// 4. Falls back to sentence-boundary splitting with sentence-level overlap instead of mid-word slicing.
pub fn chunk_markdown(
    doc_id_prefix: &str,
    default_doc_title: Option<&str>,
    category: &str,
    markdown: &str,
    max_chunk_chars: usize,
    overlap_chars: usize,
) -> Vec<KnowledgeChunk> {
    let max_chars = if max_chunk_chars == 0 {
        DEFAULT_MAX_CHUNK_CHARS
    } else {
        max_chunk_chars
    };
    let overlap = overlap_chars.min(max_chars / 2);

    let fallback_title = default_doc_title
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(doc_id_prefix);

    let sections = split_into_heading_sections(markdown, fallback_title);

    let mut chunks = Vec::new();
    let mut index = 1usize;

    for (breadcrumb_title, body) in sections {
        let sub_texts = split_section_by_context(&body, max_chars, overlap);
        for sub in sub_texts {
            let id = format!("{doc_id_prefix}-{index}");
            index += 1;
            chunks.push(KnowledgeChunk {
                id,
                doc_title: breadcrumb_title.clone(),
                category: category.to_string(),
                content: sub,
            });
        }
    }

    chunks
}

fn split_into_heading_sections(markdown: &str, fallback_title: &str) -> Vec<(String, String)> {
    let mut heading_stack: Vec<(usize, String)> = Vec::new();
    let mut current_lines: Vec<&str> = Vec::new();
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut in_fence: Option<char> = None;

    for line in markdown.lines() {
        let trimmed = line.trim();

        if let Some(fence_char) = detect_fence_char(trimmed) {
            match in_fence {
                Some(active) if active == fence_char => in_fence = None,
                None => in_fence = Some(fence_char),
                _ => {}
            }
            current_lines.push(line);
            continue;
        }

        if in_fence.is_none()
            && let Some((level, heading_text)) = parse_markdown_heading(trimmed)
        {
            let body = current_lines.join("\n").trim().to_string();
            if !body.is_empty() {
                let title = format_breadcrumb(&heading_stack, fallback_title);
                sections.push((title, body));
            }
            current_lines.clear();

            while let Some((top_level, _)) = heading_stack.last() {
                if *top_level >= level {
                    heading_stack.pop();
                } else {
                    break;
                }
            }
            heading_stack.push((level, heading_text.to_string()));
            continue;
        }

        current_lines.push(line);
    }

    let remaining = current_lines.join("\n").trim().to_string();
    if !remaining.is_empty() {
        let title = format_breadcrumb(&heading_stack, fallback_title);
        sections.push((title, remaining));
    }

    sections
}

fn format_breadcrumb(stack: &[(usize, String)], fallback: &str) -> String {
    if stack.is_empty() {
        return fallback.to_string();
    }
    stack
        .iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join(" > ")
}

fn detect_fence_char(trimmed_line: &str) -> Option<char> {
    if trimmed_line.starts_with("```") {
        Some('`')
    } else if trimmed_line.starts_with("~~~") {
        Some('~')
    } else {
        None
    }
}

fn parse_markdown_heading(line: &str) -> Option<(usize, &str)> {
    if !line.starts_with('#') {
        return None;
    }
    let level = line.chars().take_while(|&c| c == '#').count();
    if (1..=4).contains(&level) {
        let rest = &line[level..];
        if rest.starts_with(' ') {
            let title = rest.trim();
            if !title.is_empty() {
                return Some((level, title));
            }
        }
    }
    None
}

/// Group Markdown lines into atomic blocks (fenced code blocks stay intact,
/// tables stay intact, paragraphs/lists split on blank lines).
fn extract_atomic_blocks(section: &str) -> Vec<String> {
    let mut blocks: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut in_fence: Option<char> = None;

    for line in section.lines() {
        let trimmed = line.trim();

        if let Some(fence_char) = detect_fence_char(trimmed) {
            match in_fence {
                Some(active) if active == fence_char => {
                    current.push(line);
                    in_fence = None;
                    let block = current.join("\n").trim().to_string();
                    if !block.is_empty() {
                        blocks.push(block);
                    }
                    current.clear();
                    continue;
                }
                None => {
                    // Flush any text preceding the code block
                    let preceding = current.join("\n").trim().to_string();
                    if !preceding.is_empty() {
                        blocks.push(preceding);
                    }
                    current.clear();
                    in_fence = Some(fence_char);
                    current.push(line);
                    continue;
                }
                _ => {}
            }
        }

        if in_fence.is_some() {
            current.push(line);
            continue;
        }

        if trimmed.is_empty() {
            let block = current.join("\n").trim().to_string();
            if !block.is_empty() {
                blocks.push(block);
            }
            current.clear();
        } else {
            current.push(line);
        }
    }

    let tail = current.join("\n").trim().to_string();
    if !tail.is_empty() {
        blocks.push(tail);
    }

    blocks
}

fn split_section_by_context(section: &str, max_chars: usize, overlap_chars: usize) -> Vec<String> {
    let trimmed = section.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if trimmed.chars().count() <= max_chars {
        return vec![trimmed.to_string()];
    }

    let blocks = extract_atomic_blocks(trimmed);
    let mut results: Vec<String> = Vec::new();
    let mut current = String::new();

    for block in blocks {
        let block_len = block.chars().count();
        let current_len = current.chars().count();

        if block_len > max_chars {
            // Flush current accumulator first
            if !current.trim().is_empty() {
                results.push(current.trim().to_string());
                current.clear();
            }

            // Keep fenced code blocks intact if possible; split oversized prose/lists by lines/sentences
            if block.starts_with("```") || block.starts_with("~~~") {
                results.push(block);
            } else {
                let sub_chunks = split_oversized_block(&block, max_chars, overlap_chars);
                results.extend(sub_chunks);
            }
            continue;
        }

        if current.is_empty() {
            current = block;
        } else if current_len + block_len + 2 <= max_chars {
            current.push_str("\n\n");
            current.push_str(&block);
        } else {
            results.push(current.trim().to_string());
            let overlap_prefix = extract_sentence_overlap(&current, overlap_chars);
            if !overlap_prefix.is_empty()
                && overlap_prefix.chars().count() + block_len + 2 <= max_chars
            {
                current = format!("{overlap_prefix}\n\n{block}");
            } else {
                current = block;
            }
        }
    }

    if !current.trim().is_empty() {
        results.push(current.trim().to_string());
    }

    results
}

fn split_oversized_block(block: &str, max_chars: usize, overlap_chars: usize) -> Vec<String> {
    // Prefer splitting by lines first (ideal for Markdown bullet lists or tables)
    let lines: Vec<&str> = block
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();

    if lines.len() > 1 {
        let mut results = Vec::new();
        let mut current = String::new();
        for line in lines {
            let line_len = line.chars().count();
            if line_len > max_chars {
                if !current.trim().is_empty() {
                    results.push(current.trim().to_string());
                    current.clear();
                }
                results.extend(split_by_sentences(line, max_chars, overlap_chars));
                continue;
            }
            if current.is_empty() {
                current = line.to_string();
            } else if current.chars().count() + line_len < max_chars {
                current.push('\n');
                current.push_str(line);
            } else {
                results.push(current.trim().to_string());
                current = line.to_string();
            }
        }
        if !current.trim().is_empty() {
            results.push(current.trim().to_string());
        }
        return results;
    }

    split_by_sentences(block, max_chars, overlap_chars)
}

fn split_by_sentences(text: &str, max_chars: usize, overlap_chars: usize) -> Vec<String> {
    let sentences = tokenize_sentences(text);
    if sentences.is_empty() {
        return Vec::new();
    }

    let mut results: Vec<String> = Vec::new();
    let mut current_sentences: Vec<String> = Vec::new();
    let mut current_len = 0usize;

    for sentence in sentences {
        let s_len = sentence.chars().count();

        if s_len > max_chars {
            if !current_sentences.is_empty() {
                results.push(current_sentences.join(" "));
                current_sentences.clear();
                current_len = 0;
            }
            results.extend(split_by_words(&sentence, max_chars, overlap_chars));
            continue;
        }

        let added_len = if current_sentences.is_empty() {
            s_len
        } else {
            s_len + 1
        };

        if current_len + added_len <= max_chars {
            current_sentences.push(sentence);
            current_len += added_len;
        } else {
            results.push(current_sentences.join(" "));

            // Retain trailing sentences that fit within overlap_chars
            let mut overlap_buf: Vec<String> = Vec::new();
            let mut overlap_len = 0usize;
            for prev in current_sentences.iter().rev() {
                let p_len = prev.chars().count();
                let next_len = if overlap_buf.is_empty() {
                    p_len
                } else {
                    overlap_len + p_len + 1
                };
                if next_len <= overlap_chars && next_len + s_len < max_chars {
                    overlap_buf.push(prev.clone());
                    overlap_len = next_len;
                } else {
                    break;
                }
            }
            overlap_buf.reverse();
            overlap_buf.push(sentence);

            current_len = overlap_buf.iter().map(|s| s.chars().count()).sum::<usize>()
                + overlap_buf.len().saturating_sub(1);
            current_sentences = overlap_buf;
        }
    }

    if !current_sentences.is_empty() {
        results.push(current_sentences.join(" "));
    }

    results
}

fn tokenize_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        current.push(c);
        if matches!(c, '.' | '?' | '!' | ';') {
            let next_is_boundary = chars
                .peek()
                .map(|next| next.is_whitespace())
                .unwrap_or(true);
            if next_is_boundary {
                let s = current.trim().to_string();
                if !s.is_empty() {
                    sentences.push(s);
                }
                current.clear();
            }
        }
    }

    let tail = current.trim().to_string();
    if !tail.is_empty() {
        sentences.push(tail);
    }

    sentences
}

fn split_by_words(text: &str, max_chars: usize, overlap_chars: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut results = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut current_len = 0usize;

    for word in words {
        let w_len = word.chars().count();
        let added = if current.is_empty() { w_len } else { w_len + 1 };

        if !current.is_empty() && current_len + added > max_chars {
            results.push(current.join(" "));

            let mut overlap_words: Vec<&str> = Vec::new();
            let mut overlap_len = 0usize;
            for &prev in current.iter().rev() {
                let p_len = prev.chars().count();
                let next_len = if overlap_words.is_empty() {
                    p_len
                } else {
                    overlap_len + p_len + 1
                };
                if next_len <= overlap_chars && next_len + w_len < max_chars {
                    overlap_words.push(prev);
                    overlap_len = next_len;
                } else {
                    break;
                }
            }
            overlap_words.reverse();
            overlap_words.push(word);
            current_len = overlap_words
                .iter()
                .map(|w| w.chars().count())
                .sum::<usize>()
                + overlap_words.len().saturating_sub(1);
            current = overlap_words;
        } else {
            current.push(word);
            current_len += added;
        }
    }

    if !current.is_empty() {
        results.push(current.join(" "));
    }

    results
}

fn extract_sentence_overlap(text: &str, overlap_chars: usize) -> String {
    if overlap_chars == 0 {
        return String::new();
    }
    // Avoid duplicating code blocks as overlap
    if text.trim_end().ends_with("```") || text.trim_end().ends_with("~~~") {
        return String::new();
    }

    let sentences = tokenize_sentences(text);
    let mut selected: Vec<String> = Vec::new();
    let mut total_len = 0usize;

    for sentence in sentences.iter().rev() {
        let s_len = sentence.chars().count();
        let next_len = if selected.is_empty() {
            s_len
        } else {
            total_len + s_len + 1
        };
        if next_len <= overlap_chars {
            selected.push(sentence.clone());
            total_len = next_len;
        } else {
            break;
        }
    }

    selected.reverse();
    selected.join(" ")
}
