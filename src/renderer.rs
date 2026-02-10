use anyhow::{Context, Result};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct RenderedDoc {
    pub lines: Vec<Line<'static>>,
    pub images: Vec<RenderedImage>,
}

#[derive(Debug, Clone)]
pub struct RenderedImage {
    pub src: String,
    pub line_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Ordered(u64),
    Unordered,
}

pub fn render_markdown(input: &str) -> Result<RenderedDoc> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(input, options);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut images: Vec<RenderedImage> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();

    let mut style_stack = vec![Style::default()];
    let mut list_stack: Vec<ListKind> = Vec::new();
    let mut in_code_block = false;
    let mut in_blockquote = 0usize;
    let mut pending_link: Option<String> = None;
    let mut pending_image: Option<(String, String)> = None;
    let soft_break_as_space = true;

    fn push_line(lines: &mut Vec<Line<'static>>, spans: &mut Vec<Span<'static>>) {
        if spans.is_empty() {
            lines.push(Line::default());
        } else {
            lines.push(Line::from(std::mem::take(spans)));
        }
    }

    fn blank_line(lines: &mut Vec<Line<'static>>, spans: &mut Vec<Span<'static>>) {
        if !spans.is_empty() {
            push_line(lines, spans);
        }
        if !lines.last().map(|l| l.spans.is_empty()).unwrap_or(false) {
            lines.push(Line::default());
        }
    }

    fn indent_for_lists(list_stack: &[ListKind]) -> String {
        if list_stack.is_empty() {
            return String::new();
        }
        "  ".repeat(list_stack.len().saturating_sub(1))
    }

    fn append_image_entry(
        lines: &mut Vec<Line<'static>>,
        images: &mut Vec<RenderedImage>,
        src: String,
        alt_raw: String,
        trailing_blank: bool,
    ) {
        let alt = alt_raw.trim().to_string();
        let line_index = lines.len();
        let mut spans = Vec::new();
        spans.push(Span::styled(
            "[image] ".to_string(),
            Style::default().add_modifier(Modifier::DIM | Modifier::BOLD),
        ));
        if alt.is_empty() {
            spans.push(Span::styled(
                src.clone(),
                Style::default().add_modifier(Modifier::UNDERLINED),
            ));
        } else {
            spans.push(Span::raw(alt));
            spans.push(Span::styled(
                format!(" ({src})"),
                Style::default().add_modifier(Modifier::DIM),
            ));
        }
        lines.push(Line::from(spans));
        images.push(RenderedImage { src, line_index });
        if trailing_blank {
            lines.push(Line::default());
        }
    }

    for event in parser {
        if pending_image.is_some() {
            match event {
                Event::End(TagEnd::Image) => {
                    let (src, alt_raw) = pending_image.take().expect("pending image exists");
                    append_image_entry(&mut lines, &mut images, src, alt_raw, true);
                }
                Event::Text(text)
                | Event::Code(text)
                | Event::Html(text)
                | Event::InlineHtml(text)
                | Event::InlineMath(text)
                | Event::DisplayMath(text) => {
                    if let Some((_, alt)) = pending_image.as_mut() {
                        alt.push_str(&text);
                    }
                }
                Event::SoftBreak | Event::HardBreak => {
                    if let Some((_, alt)) = pending_image.as_mut() {
                        alt.push(' ');
                    }
                }
                _ => {}
            }
            continue;
        }

        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {}
                Tag::Heading { level, .. } => {
                    blank_line(&mut lines, &mut current_spans);
                    let style = match level {
                        HeadingLevel::H1 => {
                            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                        }
                        HeadingLevel::H2 => Style::default().add_modifier(Modifier::BOLD),
                        HeadingLevel::H3 | HeadingLevel::H4 => {
                            Style::default().add_modifier(Modifier::BOLD | Modifier::ITALIC)
                        }
                        HeadingLevel::H5 | HeadingLevel::H6 => {
                            Style::default().add_modifier(Modifier::ITALIC)
                        }
                    };
                    style_stack.push(style);
                }
                Tag::BlockQuote(_) => {
                    in_blockquote += 1;
                    if current_spans.is_empty() {
                        let prefix = format!("{}│ ", "  ".repeat(in_blockquote.saturating_sub(1)));
                        current_spans.push(Span::styled(
                            prefix,
                            Style::default().add_modifier(Modifier::DIM),
                        ));
                    }
                }
                Tag::CodeBlock(kind) => {
                    blank_line(&mut lines, &mut current_spans);
                    in_code_block = true;
                    let code_block_lang = match kind {
                        CodeBlockKind::Fenced(lang) => {
                            let lang = lang.trim();
                            if lang.is_empty() {
                                None
                            } else {
                                Some(lang.to_string())
                            }
                        }
                        CodeBlockKind::Indented => None,
                    };
                    let header = match &code_block_lang {
                        Some(lang) => format!("```{lang}"),
                        None => "```".to_string(),
                    };
                    lines.push(Line::from(Span::styled(
                        header,
                        Style::default().add_modifier(Modifier::DIM),
                    )));
                }
                Tag::List(start) => {
                    let kind = match start {
                        Some(v) => ListKind::Ordered(v),
                        None => ListKind::Unordered,
                    };
                    list_stack.push(kind);
                }
                Tag::Item => {
                    if !current_spans.is_empty() {
                        push_line(&mut lines, &mut current_spans);
                    }
                    let indent = indent_for_lists(&list_stack);
                    let marker = match list_stack.last_mut() {
                        Some(ListKind::Ordered(n)) => {
                            let m = format!("{n}. ");
                            *n += 1;
                            m
                        }
                        Some(ListKind::Unordered) | None => "• ".to_string(),
                    };
                    current_spans.push(Span::raw(format!("{indent}{marker}")));
                }
                Tag::Emphasis => {
                    let base = *style_stack.last().unwrap_or(&Style::default());
                    style_stack.push(base.add_modifier(Modifier::ITALIC));
                }
                Tag::Strong => {
                    let base = *style_stack.last().unwrap_or(&Style::default());
                    style_stack.push(base.add_modifier(Modifier::BOLD));
                }
                Tag::Strikethrough => {
                    let base = *style_stack.last().unwrap_or(&Style::default());
                    style_stack.push(base.add_modifier(Modifier::CROSSED_OUT));
                }
                Tag::Link { dest_url, .. } => {
                    let base = *style_stack.last().unwrap_or(&Style::default());
                    style_stack.push(base.add_modifier(Modifier::UNDERLINED));
                    pending_link = Some(dest_url.to_string());
                }
                Tag::Image { dest_url, .. } => {
                    blank_line(&mut lines, &mut current_spans);
                    pending_image = Some((dest_url.to_string(), String::new()));
                }
                Tag::Table(_) => {
                    blank_line(&mut lines, &mut current_spans);
                }
                Tag::TableHead => {}
                Tag::TableRow => {}
                Tag::TableCell => {
                    if !current_spans.is_empty() {
                        current_spans.push(Span::raw(" │ ".to_string()));
                    }
                }
                Tag::FootnoteDefinition(name) => {
                    blank_line(&mut lines, &mut current_spans);
                    current_spans.push(Span::styled(
                        format!("[^{name}] "),
                        Style::default().add_modifier(Modifier::DIM),
                    ));
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    push_line(&mut lines, &mut current_spans);
                    lines.push(Line::default());
                }
                TagEnd::Heading(_) => {
                    push_line(&mut lines, &mut current_spans);
                    style_stack.pop();
                    lines.push(Line::default());
                }
                TagEnd::BlockQuote(_) => {
                    if !current_spans.is_empty() {
                        push_line(&mut lines, &mut current_spans);
                    }
                    in_blockquote = in_blockquote.saturating_sub(1);
                    lines.push(Line::default());
                }
                TagEnd::CodeBlock => {
                    if !current_spans.is_empty() {
                        push_line(&mut lines, &mut current_spans);
                    }
                    lines.push(Line::from(Span::styled(
                        "```".to_string(),
                        Style::default().add_modifier(Modifier::DIM),
                    )));
                    lines.push(Line::default());
                    in_code_block = false;
                }
                TagEnd::List(_) => {
                    list_stack.pop();
                    lines.push(Line::default());
                }
                TagEnd::Item => {
                    push_line(&mut lines, &mut current_spans);
                }
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                    style_stack.pop();
                }
                TagEnd::Link => {
                    style_stack.pop();
                    if let Some(link) = pending_link.take() {
                        current_spans.push(Span::styled(
                            format!(" ({link})"),
                            Style::default().add_modifier(Modifier::DIM),
                        ));
                    }
                }
                TagEnd::Image => {}
                TagEnd::Table => {
                    if !current_spans.is_empty() {
                        push_line(&mut lines, &mut current_spans);
                    }
                    lines.push(Line::default());
                }
                TagEnd::TableRow => {
                    push_line(&mut lines, &mut current_spans);
                }
                TagEnd::TableCell => {}
                _ => {}
            },
            Event::Text(text) => {
                let base = *style_stack.last().unwrap_or(&Style::default());
                if in_code_block {
                    let text = text.to_string();
                    for line in text.split('\n') {
                        if !line.is_empty() {
                            current_spans.push(Span::styled(
                                format!("  {line}"),
                                base.add_modifier(Modifier::DIM),
                            ));
                        }
                        push_line(&mut lines, &mut current_spans);
                    }
                } else {
                    if current_spans.is_empty() && in_blockquote > 0 {
                        let prefix = format!("{}│ ", "  ".repeat(in_blockquote.saturating_sub(1)));
                        current_spans.push(Span::styled(
                            prefix,
                            Style::default().add_modifier(Modifier::DIM),
                        ));
                    }
                    current_spans.push(Span::styled(text.to_string(), base));
                }
            }
            Event::Code(text) => {
                let base = *style_stack.last().unwrap_or(&Style::default());
                current_spans.push(Span::styled(
                    format!("`{text}`"),
                    base.add_modifier(Modifier::BOLD),
                ));
            }
            Event::Html(text) => {
                let html = text.to_string();
                let html_images = extract_html_images(&html);
                if html_images.is_empty() {
                    current_spans.push(Span::styled(
                        html,
                        Style::default().add_modifier(Modifier::DIM),
                    ));
                } else {
                    blank_line(&mut lines, &mut current_spans);
                    for (src, alt) in html_images {
                        append_image_entry(&mut lines, &mut images, src, alt, true);
                    }
                }
            }
            Event::SoftBreak => {
                if soft_break_as_space {
                    current_spans.push(Span::raw(" ".to_string()));
                } else {
                    push_line(&mut lines, &mut current_spans);
                }
            }
            Event::HardBreak => {
                push_line(&mut lines, &mut current_spans);
            }
            Event::Rule => {
                push_line(&mut lines, &mut current_spans);
                lines.push(Line::from(Span::styled(
                    "─".repeat(72),
                    Style::default().add_modifier(Modifier::DIM),
                )));
            }
            Event::TaskListMarker(done) => {
                let marker = if done { "[x] " } else { "[ ] " };
                current_spans.push(Span::styled(
                    marker.to_string(),
                    Style::default().add_modifier(Modifier::BOLD),
                ));
            }
            Event::FootnoteReference(name) => {
                current_spans.push(Span::styled(
                    format!("[^{name}]"),
                    Style::default().add_modifier(Modifier::DIM),
                ));
            }
            Event::InlineMath(text) | Event::DisplayMath(text) => {
                current_spans.push(Span::styled(
                    format!("${text}$"),
                    Style::default().add_modifier(Modifier::ITALIC),
                ));
            }
            Event::InlineHtml(text) => {
                let html = text.to_string();
                let html_images = extract_html_images(&html);
                if html_images.is_empty() {
                    current_spans.push(Span::styled(
                        html,
                        Style::default().add_modifier(Modifier::DIM),
                    ));
                } else {
                    blank_line(&mut lines, &mut current_spans);
                    for (src, alt) in html_images {
                        append_image_entry(&mut lines, &mut images, src, alt, true);
                    }
                }
            }
        }
    }

    if !current_spans.is_empty() {
        push_line(&mut lines, &mut current_spans);
    }

    if let Some((src, alt_raw)) = pending_image.take() {
        append_image_entry(&mut lines, &mut images, src, alt_raw, false);
    }

    while lines.last().map(|l| l.spans.is_empty()).unwrap_or(false) {
        lines.pop();
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "(empty markdown file)".to_string(),
            Style::default().add_modifier(Modifier::DIM),
        )));
    }

    Ok(RenderedDoc { lines, images })
}

pub fn read_markdown_file(path: &std::path::Path) -> Result<String> {
    std::fs::read_to_string(path)
        .with_context(|| format!("failed to read file: {}", path.display()))
}

fn extract_html_images(html: &str) -> Vec<(String, String)> {
    static IMG_TAG_RE: OnceLock<Regex> = OnceLock::new();
    static SRC_RE: OnceLock<Regex> = OnceLock::new();
    static ALT_RE: OnceLock<Regex> = OnceLock::new();

    let img_tag_re = IMG_TAG_RE
        .get_or_init(|| Regex::new(r#"(?is)<img\b[^>]*>"#).expect("valid image tag regex"));
    let src_re = SRC_RE.get_or_init(|| {
        Regex::new(r#"(?is)\bsrc\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+))"#)
            .expect("valid src regex")
    });
    let alt_re = ALT_RE.get_or_init(|| {
        Regex::new(r#"(?is)\balt\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+))"#)
            .expect("valid alt regex")
    });

    img_tag_re
        .find_iter(html)
        .filter_map(|m| {
            let tag = m.as_str();
            let src = src_re
                .captures(tag)
                .and_then(|caps| first_non_empty_capture_owned(&caps))?;
            let alt = alt_re
                .captures(tag)
                .and_then(|caps| first_non_empty_capture_owned(&caps))
                .unwrap_or_default();
            Some((src, alt))
        })
        .collect()
}

fn first_non_empty_capture_owned(caps: &regex::Captures<'_>) -> Option<String> {
    (1..caps.len())
        .filter_map(|idx| caps.get(idx))
        .map(|m| m.as_str())
        .find(|value| !value.is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_images_are_extracted_for_runtime_rendering() {
        let doc = render_markdown("![Alt Text](images/sample.png)").expect("render succeeds");
        assert_eq!(doc.images.len(), 1);
        assert_eq!(doc.images[0].src, "images/sample.png");

        let caption_line = &doc.lines[doc.images[0].line_index];
        let caption_text = caption_line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert!(caption_text.contains("[image]"));
    }

    #[test]
    fn html_img_tags_are_extracted_for_runtime_rendering() {
        let input = r#"<img alt="Preview" src="https://example.com/preview.png" />"#;
        let doc = render_markdown(input).expect("render succeeds");
        assert_eq!(doc.images.len(), 1);
        assert_eq!(doc.images[0].src, "https://example.com/preview.png");

        let caption_line = &doc.lines[doc.images[0].line_index];
        let caption_text = caption_line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert!(caption_text.contains("Preview"));
    }
}
