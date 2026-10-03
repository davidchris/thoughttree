use crate::GraphNode;

/// JavaScript's existing Palette uses literal `/token/i` matches without the
/// Unicode flag. Match its single-code-unit folding, rather than Rust regex's
/// broader Unicode folding (which also equates Kelvin K and long S with ASCII).
struct TokenMatcher(Vec<char>);

impl TokenMatcher {
    fn new(token: &str) -> Self {
        Self(token.chars().map(canonicalize).collect())
    }

    fn spans(&self, text: &str) -> Vec<TextSpan> {
        let characters: Vec<_> = text.char_indices().collect();
        let mut spans = Vec::new();
        let mut index = 0;
        while index + self.0.len() <= characters.len() {
            if characters[index..index + self.0.len()]
                .iter()
                .map(|(_, ch)| canonicalize(*ch))
                .eq(self.0.iter().copied())
            {
                spans.push(TextSpan {
                    start: characters[index].0,
                    end: characters
                        .get(index + self.0.len())
                        .map_or(text.len(), |ch| ch.0),
                });
                index += self.0.len();
            } else {
                index += 1;
            }
        }
        spans
    }
}

fn canonicalize(ch: char) -> char {
    if ch.len_utf16() != 1 {
        return ch;
    }
    let mut uppercase = ch.to_uppercase();
    let first = uppercase.next().unwrap_or(ch);
    if uppercase.next().is_some() || (!ch.is_ascii() && first.is_ascii()) {
        ch
    } else {
        first
    }
}

fn is_space(ch: char) -> bool {
    matches!(ch, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSpan {
    /// UTF-8 byte offsets suitable for GPUI's text highlighting APIs.
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightedText {
    pub text: String,
    pub spans: Vec<TextSpan>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    pub node: GraphNode,
    pub title: HighlightedText,
    pub snippet: Option<HighlightedText>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    pub total: usize,
}

struct Match<'a> {
    node: &'a GraphNode,
    summary_matched: bool,
    first_content_pos: Option<usize>,
}

/// Opening a Palette captures an immutable corpus. Streaming and later edits
/// cannot reorder these search results while the user is choosing a node.
#[derive(Clone, Debug)]
pub struct CorpusSnapshot {
    nodes: Vec<GraphNode>,
}

impl CorpusSnapshot {
    pub fn new(nodes: impl IntoIterator<Item = GraphNode>) -> Self {
        Self {
            nodes: nodes.into_iter().collect(),
        }
    }
    pub fn search(&self, query: &str, limit: usize) -> SearchResult {
        search_nodes(&self.nodes, query, limit)
    }
}

pub fn search_nodes(corpus: &[GraphNode], query: &str, limit: usize) -> SearchResult {
    let matchers: Vec<_> = query
        .split(is_space)
        .filter(|token| !token.is_empty())
        .map(TokenMatcher::new)
        .collect();
    let mut matches: Vec<_> = corpus
        .iter()
        .filter_map(|node| match_node(node, &matchers))
        .collect();
    matches.sort_by(|a, b| {
        b.summary_matched
            .cmp(&a.summary_matched)
            .then(
                a.first_content_pos
                    .unwrap_or(usize::MAX)
                    .cmp(&b.first_content_pos.unwrap_or(usize::MAX)),
            )
            .then(recency(b.node).total_cmp(&recency(a.node)))
    });
    let total = matches.len();
    let hits = matches
        .into_iter()
        .take(limit)
        .map(|entry| hit(entry, &matchers))
        .collect();
    SearchResult { hits, total }
}

fn searchable(node: &GraphNode) -> &str {
    node.file_data()
        .map_or(node.content.as_str(), |file| file.path.as_str())
}
fn summary(node: &GraphNode) -> &str {
    if node.file_data().is_some() {
        ""
    } else {
        node.summary.as_deref().unwrap_or_default()
    }
}
fn recency(node: &GraphNode) -> f64 {
    if node.file_data().is_some() {
        node.timestamp
    } else {
        node.content_updated_at.unwrap_or(node.timestamp)
    }
}

fn match_node<'a>(node: &'a GraphNode, matchers: &[TokenMatcher]) -> Option<Match<'a>> {
    let content = searchable(node);
    let summary = summary(node);
    if content.is_empty() && summary.is_empty() {
        return None;
    }
    let mut first_content_pos = None;
    let mut summary_matched = false;
    for matcher in matchers {
        let content_match = matcher.spans(content).into_iter().next();
        let in_summary = !matcher.spans(summary).is_empty();
        if content_match.is_none() && !in_summary {
            return None;
        }
        if let Some(found) = content_match {
            let position = content[..found.start].encode_utf16().count();
            first_content_pos =
                Some(first_content_pos.map_or(position, |pos: usize| pos.min(position)));
        }
        summary_matched |= in_summary;
    }
    Some(Match {
        node,
        summary_matched,
        first_content_pos,
    })
}

fn highlighted(text: String, matchers: &[TokenMatcher]) -> HighlightedText {
    let mut raw: Vec<_> = matchers
        .iter()
        .flat_map(|matcher| matcher.spans(&text))
        .collect();
    raw.sort_by_key(|span| (span.start, span.end));
    let mut spans: Vec<TextSpan> = Vec::new();
    for span in raw {
        if let Some(last) = spans.last_mut().filter(|last| span.start <= last.end) {
            last.end = last.end.max(span.end);
        } else {
            spans.push(span);
        }
    }
    HighlightedText { text, spans }
}

fn hit(entry: Match<'_>, matchers: &[TokenMatcher]) -> SearchHit {
    let title = highlighted(title(entry.node), matchers);
    let snippet = if let Some(file) = entry.node.file_data() {
        Some(highlighted(file.path.clone(), matchers))
    } else {
        entry
            .first_content_pos
            .map(|position| snippet(&entry.node.content, position, matchers))
    };
    SearchHit {
        node: entry.node.clone(),
        title,
        snippet,
    }
}

fn title(node: &GraphNode) -> String {
    if let Some(file) = node.file_data() {
        return slice_utf16(&file.name, 0, 80);
    }
    if let Some(summary) = node.summary.as_ref().filter(|summary| !summary.is_empty()) {
        return summary.clone();
    }
    let first = node
        .content
        .trim_start_matches(is_space)
        .split('\n')
        .next()
        .unwrap_or_default();
    slice_utf16(first, 0, 80)
}

/// UTF-16 display limits match the browser while never cutting a code point.
fn slice_utf16(text: &str, start: usize, end: usize) -> String {
    let mut position = 0;
    text.chars()
        .filter(|ch| {
            let from = position;
            position += ch.len_utf16();
            from >= start && position <= end
        })
        .collect()
}

fn snippet(content: &str, first: usize, matchers: &[TokenMatcher]) -> HighlightedText {
    let start = first
        .saturating_sub(30)
        .min(content.encode_utf16().count().saturating_sub(100));
    let mut was_space = false;
    let text = slice_utf16(content, start, start + 100)
        .chars()
        .filter_map(|ch| {
            let space = is_space(ch);
            let keep = !space || !was_space;
            was_space = space;
            keep.then_some(if space { ' ' } else { ch })
        })
        .collect();
    highlighted(text, matchers)
}
