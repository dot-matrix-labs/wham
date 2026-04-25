//! Text node element — static text content with a semantic variant.
//!
//! ARIA role: depends on variant (`heading`, `paragraph`, or none)

/// The semantic variant of a text node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextVariant {
    /// Plain paragraph text (default).
    #[default]
    Body,
    /// A heading (maps to `<h1>`–`<h6>`). The `level` field (1–6) further
    /// qualifies the heading rank.
    Heading {
        /// Heading level 1 (most important) through 6 (least important).
        level: u8,
    },
    /// Smaller, de-emphasised text (e.g. captions, metadata).
    Muted,
}

/// A non-interactive text node.
///
/// # ARIA role: `heading` for `TextVariant::Heading`, implicit for others
#[derive(Clone, Debug)]
pub struct TextNode {
    /// Text content.
    pub content: String,
    /// Semantic variant that drives visual and ARIA treatment.
    pub variant: TextVariant,
}

impl TextNode {
    /// Create a plain body text node.
    pub fn body(content: impl Into<String>) -> Self {
        Self { content: content.into(), variant: TextVariant::Body }
    }

    /// Create a heading text node.
    pub fn heading(content: impl Into<String>, level: u8) -> Self {
        let level = level.clamp(1, 6);
        Self { content: content.into(), variant: TextVariant::Heading { level } }
    }

    /// Create a muted (de-emphasised) text node.
    pub fn muted(content: impl Into<String>) -> Self {
        Self { content: content.into(), variant: TextVariant::Muted }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_text() {
        let t = TextNode::body("Hello");
        assert_eq!(t.content, "Hello");
        assert_eq!(t.variant, TextVariant::Body);
    }

    #[test]
    fn heading_text() {
        let t = TextNode::heading("Title", 2);
        assert_eq!(t.content, "Title");
        assert_eq!(t.variant, TextVariant::Heading { level: 2 });
    }

    #[test]
    fn heading_level_clamped() {
        let t = TextNode::heading("X", 0);
        assert_eq!(t.variant, TextVariant::Heading { level: 1 });
        let t2 = TextNode::heading("X", 9);
        assert_eq!(t2.variant, TextVariant::Heading { level: 6 });
    }

    #[test]
    fn muted_text() {
        let t = TextNode::muted("Small print");
        assert_eq!(t.variant, TextVariant::Muted);
    }
}
