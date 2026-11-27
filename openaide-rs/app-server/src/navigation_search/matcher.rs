//! Case-insensitive literal matching and snippet windows.
//!
//! Folding maps each `char` to exactly one `char` (a lowercase form that is a single
//! character, otherwise the character itself), so a match position in folded text is the
//! same position in the original text. Reported ranges are UTF-16 code units, which is
//! what a JavaScript client slices by.

use openaide_app_server_protocol::task::{SearchSnippet, TextHighlight};

/// Characters of context kept before the first match in a snippet.
const LEAD_CONTEXT: usize = 32;
/// Soft cap on a snippet window; a longer query widens it so the match always fits.
const WINDOW: usize = 140;
const ELLIPSIS: char = '\u{2026}';

pub(crate) fn fold(character: char) -> char {
    let mut lowered = character.to_lowercase();
    match (lowered.next(), lowered.next()) {
        (Some(single), None) => single,
        _ => character,
    }
}

/// A prepared query. Constructing it trims the text; an empty query matches nothing.
pub(crate) struct Matcher {
    folded: Vec<char>,
}

/// Reusable buffers for one scan, so a long Chat allocates once, not once per message.
#[derive(Default)]
pub(crate) struct Scratch {
    original: Vec<char>,
    folded: Vec<char>,
}

impl Scratch {
    fn load(&mut self, text: &str) {
        self.original.clear();
        self.folded.clear();
        for character in text.chars() {
            self.folded.push(fold(character));
            // Control characters and newlines would break a one-line row.
            self.original
                .push(if character.is_whitespace() || character.is_control() {
                    ' '
                } else {
                    character
                });
        }
    }
}

impl Matcher {
    pub(crate) fn new(query: &str) -> Option<Self> {
        let folded: Vec<char> = query.trim().chars().map(fold).collect();
        (!folded.is_empty()).then_some(Self { folded })
    }

    fn find_from(&self, haystack: &[char], from: usize) -> Option<usize> {
        let needle = &self.folded;
        if haystack.len() < needle.len() {
            return None;
        }
        let last = haystack.len() - needle.len();
        let first = needle[0];
        let mut index = from;
        while index <= last {
            if haystack[index] == first && haystack[index..index + needle.len()] == needle[..] {
                return Some(index);
            }
            index += 1;
        }
        None
    }

    /// Every non-overlapping match in `title`, as UTF-16 ranges; empty when it does not match.
    pub(crate) fn title_highlights(&self, title: &str) -> Vec<TextHighlight> {
        let mut scratch = Scratch::default();
        scratch.load(title);
        let units = utf16_prefix(&scratch.original);
        self.ranges_in(&scratch.folded, 0, scratch.folded.len(), &units, 0)
    }

    /// The window around the first match in `text`, or `None` when it does not match.
    pub(crate) fn snippet(&self, text: &str, scratch: &mut Scratch) -> Option<SearchSnippet> {
        scratch.load(text);
        let found = self.find_from(&scratch.folded, 0)?;
        let total = scratch.folded.len();
        let width = WINDOW.max(self.folded.len() + 2 * LEAD_CONTEXT);
        let mut start = found.saturating_sub(LEAD_CONTEXT);
        let mut end = (start + width).min(total);
        // Prefer starting at a word boundary when that costs no more than a few characters.
        if start > 0 {
            if let Some(space) =
                (start..found.min(start + 12)).find(|&i| scratch.original[i] == ' ')
            {
                start = space + 1;
            }
        }
        if end < total {
            end = end.max(found + self.folded.len());
        }
        let mut shown = String::new();
        let mut units = vec![0u32];
        let mut offset = 0u32;
        let leading = start > 0;
        if leading {
            shown.push(ELLIPSIS);
            offset += 1;
        }
        let body_origin = offset;
        for &character in &scratch.original[start..end] {
            shown.push(character);
            offset += character.len_utf16() as u32;
            units.push(offset - body_origin);
        }
        if end < total {
            shown.push(ELLIPSIS);
        }
        let highlights = self.ranges_in(&scratch.folded, start, end, &units, body_origin);
        Some(SearchSnippet {
            text: shown,
            highlights,
        })
    }

    /// Matches fully inside `[start, end)`, mapped through `units` (prefix sums of UTF-16
    /// lengths relative to `start`) and shifted by `origin`.
    fn ranges_in(
        &self,
        folded: &[char],
        start: usize,
        end: usize,
        units: &[u32],
        origin: u32,
    ) -> Vec<TextHighlight> {
        let window = &folded[start..end];
        let mut ranges = Vec::new();
        let mut from = 0;
        while let Some(at) = self.find_from(window, from) {
            let stop = at + self.folded.len();
            ranges.push((origin + units[at], origin + units[stop]));
            from = stop;
        }
        ranges
    }
}

/// `result[i]` is the UTF-16 length of the first `i` characters.
fn utf16_prefix(characters: &[char]) -> Vec<u32> {
    let mut units = Vec::with_capacity(characters.len() + 1);
    let mut offset = 0;
    units.push(0);
    for character in characters {
        offset += character.len_utf16() as u32;
        units.push(offset);
    }
    units
}
