use leptos::prelude::*;
use lsp_types::{Diagnostic, Position, Range};
use shapels::analyze_source;
use std::collections::HashSet;

#[derive(Debug, Clone)]
enum Segment {
    Plain(String),
    Marked { text: String, message: String },
}

#[derive(Debug, Clone)]
struct LineRender {
    segments: Vec<Segment>,
    virtual_texts: Vec<String>,
}

fn position_to_offset(src: &str, pos: &Position) -> Option<usize> {
    let mut offset = 0usize;
    for (line_idx, line) in src.split_inclusive('\n').enumerate() {
        if line_idx as u32 == pos.line {
            let mut char_count = 0usize;
            for (byte_idx, _) in line.char_indices() {
                if char_count == pos.character as usize {
                    return Some(offset + byte_idx);
                }
                char_count += 1;
            }
            if char_count == pos.character as usize {
                return Some(offset + line.len());
            }
            return None;
        }
        offset += line.len();
    }

    // Allow positions that point to the end of the file
    if pos.line as usize == src.lines().count() && pos.character == 0 {
        return Some(src.len());
    }

    None
}

fn range_to_offsets(src: &str, range: &Range) -> Option<(usize, usize)> {
    let start = position_to_offset(src, &range.start)?;
    let end = position_to_offset(src, &range.end)?;
    Some((start.min(end), end.max(start)))
}

fn split_with_diagnostics(code: &str, diagnostics: &[Diagnostic]) -> Vec<Segment> {
    let mut ranges: Vec<(usize, usize, String)> = diagnostics
        .iter()
        .filter_map(|d| range_to_offsets(code, &d.range).map(|(s, e)| (s, e, d.message.clone())))
        .collect();

    ranges.sort_by_key(|(s, _, _)| *s);

    let mut segments = Vec::new();
    let mut cursor = 0usize;

    for (raw_start, raw_end, message) in ranges {
        if raw_start > cursor {
            segments.push(Segment::Plain(code[cursor..raw_start].to_string()));
            cursor = raw_start;
        }

        let start = raw_start.max(cursor);
        let end = raw_end.min(code.len());

        if end > start && start < code.len() {
            segments.push(Segment::Marked {
                text: code[start..end].to_string(),
                message,
            });
            cursor = end;
        }
    }

    if cursor < code.len() {
        segments.push(Segment::Plain(code[cursor..].to_string()));
    }

    // Edge case: no diagnostics
    if segments.is_empty() {
        segments.push(Segment::Plain(code.to_string()));
    }

    segments
}

fn split_lines_with_diagnostics(code: &str, diagnostics: &[Diagnostic]) -> Vec<LineRender> {
    let mut diag_ranges: Vec<(usize, usize, String)> = diagnostics
        .iter()
        .filter_map(|d| range_to_offsets(code, &d.range).map(|(s, e)| (s, e, d.message.clone())))
        .collect();
    diag_ranges.sort_by_key(|(s, _, _)| *s);

    let mut lines = Vec::new();
    let mut line_start = 0usize;

    for line in code.split('\n') {
        let line_len = line.len();
        let line_end = line_start + line_len;
        let mut overlaps: Vec<(usize, usize, String)> = diag_ranges
            .iter()
            .filter_map(|(s, e, msg)| {
                let start = (*s).max(line_start);
                let end = (*e).min(line_end);
                if start < end {
                    Some((start - line_start, end - line_start, msg.clone()))
                } else {
                    None
                }
            })
            .collect();

        overlaps.sort_by_key(|(s, _, _)| *s);

        let mut segments = Vec::new();
        let mut cursor = 0usize;
        for (s, e, msg) in overlaps {
            if s > cursor {
                segments.push(Segment::Plain(line[cursor..s].to_string()));
            }
            if e > s && s < line_len {
                segments.push(Segment::Marked {
                    text: line[s..e.min(line_len)].to_string(),
                    message: msg,
                });
                cursor = e.min(line_len);
            }
        }
        if cursor < line_len {
            segments.push(Segment::Plain(line[cursor..].to_string()));
        }
        if segments.is_empty() {
            segments.push(Segment::Plain(line.to_string()));
        }

        let mut virtual_texts = HashSet::new();
        for (s, e, msg) in diag_ranges.iter() {
            if *s < line_end && *e > line_start {
                virtual_texts.insert(msg.clone());
            }
        }

        lines.push(LineRender {
            segments,
            virtual_texts: virtual_texts.into_iter().collect(),
        });

        // account for the stripped '\n'
        line_start = line_end + 1;
    }

    // preserve trailing empty line if code ends with newline
    if code.ends_with('\n') {
        lines.push(LineRender {
            segments: vec![Segment::Plain(String::new())],
            virtual_texts: Vec::new(),
        });
    }

    lines
}

/// Code text prefilled with code, the user can modify it.
///
/// On change, it reruns the language serve, updates the hovers
/// and the diagnostics.
#[component]
fn CodeInput() -> impl IntoView {
    let initial_code = r#"
from jaxtyping import Float as F
from torch import Tensor as T
    
def matmul(x: F[T, "B X R"], y: F[T, "R S"]) -> F[T, "B S"]:
    z: F[T, "B X R"] = x @ y
    return z 
"#;

    let (code, set_code) = signal(initial_code.to_string());
    let diagnostics = Memo::new(move |_| analyze_source(&code.get()).diagnostics);
    let text_ref = NodeRef::<leptos::html::Textarea>::new();
    let overlay_ref = NodeRef::<leptos::html::Pre>::new();

    view! {
        <div class="code-wrapper">
            <pre class="code-overlay" aria-hidden="true" node_ref=overlay_ref>
                {move || {
                    split_lines_with_diagnostics(&code.get(), diagnostics.get().as_slice())
                        .into_iter()
                        .map(|line| {
                            let segments = line.segments.into_iter().map(|segment| {
                                let (text, range_class) = match segment {
                                    Segment::Plain(text) => (text, "diag-range diag-none"),
                                    Segment::Marked { text, .. } => (text, "diag-range"),
                                };
                                view! { <span class=range_class>{text}</span> }.into_view()
                            });

                            let virtuals: Vec<_> = line
                                .virtual_texts
                                .into_iter()
                                .map(|msg| {
                                    view! { <span class="diag-virtual">{" ⟫ "}{msg}</span> }
                                        .into_view()
                                })
                                .collect();

                            view! {
                                <div class="code-line">
                                    <span class="code-line-text">{segments.collect_view()}</span>
                                    <span class="diag-line-messages">{virtuals.into_iter().collect_view()}</span>
                                </div>
                            }
                            .into_view()
                        })
                        .collect_view()
                }}
            </pre>
            <textarea
                class="code-input"
                // update the signal on each keystroke
                bind:value=(code, set_code)
                spellcheck=false
                wrap="off"
                node_ref=text_ref
                on:scroll=move |_| {
                    if let (Some(textarea), Some(overlay)) = (text_ref.get(), overlay_ref.get()) {
                        overlay.set_scroll_left(textarea.scroll_left());
                        overlay.set_scroll_top(textarea.scroll_top());
                    }
                }
            ></textarea>
        </div>
    }
}
/// Default Home Page
#[component]
pub fn Home() -> impl IntoView {
    view! {
        <ErrorBoundary fallback=|errors| {
            view! {
                <h1>"Uh oh! Something went wrong!"</h1>

                <p>"Errors: "</p>
                // Render a list of errors as strings - good for development purposes
                <ul>
                    {move || {
                        errors
                            .get()
                            .into_iter()
                            .map(|(_, e)| view! { <li>{e.to_string()}</li> })
                            .collect_view()
                    }}

                </ul>
            }
        }>

            <div class="container">

                <h1>"shapels: a primer"</h1>

                <CodeInput attr:spellcheck="false"/>

            </div>
        </ErrorBoundary>
    }
}
