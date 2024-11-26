//! Code for building complete HTML pages from article bodies.

use crate::{css::Font, OUTPUT_SITE_CSS_FILE};
use anyhow::{Error, Result};
use ego_tree::{tree, NodeId, Tree};
use markup5ever::{interface::QuirksMode, namespace_url, ns, Attribute, LocalName, QualName};
use scraper::{
    node::{Doctype, Element, Node, Text},
    Html,
};

pub struct PageBuilder {
    template_html: Tree<Node>,
    head_id: NodeId,
    body_id: NodeId,
}

impl PageBuilder {
    /// Initializes a webpage HTML builder. Every page built:
    /// - includes the provided author as a metadata tag
    /// - specifies preloaded fonts based on the provided list of font sources.
    #[must_use]
    pub fn new(author: &str, site_fonts: &[Font]) -> Self {
        let mut html = Html::new_document();
        let mut root_node = html.tree.root_mut();

        // Add `<!DOCTYPE html>`
        root_node.append(Node::Doctype(Doctype {
            name: "html".into(),
            public_id: "".into(),
            system_id: "".into(),
        }));

        // Add `<html lang="en">`
        let mut html_el_node = root_node.append(create_el_with_attrs("html", &[("lang", "en")]));

        // Add `<head>` within `<html>`
        let mut head_el_node = html_el_node.append_subtree(tree! {
            create_el("head") => {
                create_el_with_attrs("meta", &[("charset", "utf-8")]),
                create_el_with_attrs("meta", &[("name", "viewport"), ("content", "width=device-width, initial-scale=1")]),
                create_el_with_attrs("meta", &[("name", "author"), ("content", author)]),
                create_el_with_attrs("link", &[("rel", "stylesheet"), ("href", OUTPUT_SITE_CSS_FILE)])
            }
        });

        // Add font `<link>`s within `<head>`
        for font in site_fonts {
            let mut attrs = Vec::with_capacity(5);
            attrs.push(("rel", "preload"));
            attrs.push(("href", &font.path));
            attrs.push(("as", "font"));
            // Preloaded fonts need to have a "crossorigin" attribute set to "anonymous"
            // even when the source is not cross-origin.
            // https://developer.mozilla.org/en-US/docs/Web/HTML/Attributes/rel/preload#cors-enabled_fetches
            attrs.push(("crossorigin", "anonymous"));

            if let Some(mime) = font.mime {
                attrs.push(("type", mime));
            }

            head_el_node.append(create_el_with_attrs("link", &attrs));
        }

        let head_id = head_el_node.id();
        let body_id = html_el_node.append(create_el("body")).id();

        Self {
            template_html: html.tree,
            head_id,
            body_id,
        }
    }

    /// Outputs a string containing a complete HTML document based on the provided document title and body.
    ///
    /// # Errors
    /// This function returns an error if the input body cannot be successfully parsed as no-quirks HTML.
    pub fn build_page(&self, title: &str, body: &str) -> Result<String> {
        let body = Html::parse_fragment(body);

        // `Html::parse_fragment()` doesn't return a `Result` because
        // the parser is supposed to be resilient and fall back to HTML quirks mode upon encountering errors.
        // So, after parsing, we have to check for any errors encountered ourselves.
        if let Some(err) = body.errors.first() {
            return Err(Error::msg(err.clone()).context("failed to parse input as valid HTML"));
        }

        let mut html = self.template_html.clone();

        // SAFETY: the ID is valid because it was generated in the constructor `PageBuilder::new()`.
        let mut head_node = unsafe { html.get_unchecked_mut(self.head_id) };
        head_node.append_subtree(tree! {
            create_el("title") => { Node::Text(Text { text: title.into() }) }
        });

        // SAFETY: the ID is valid because it was generated in the constructor `PageBuilder::new()`.
        let mut body_node = unsafe { html.get_unchecked_mut(self.body_id) };
        body_node.append_subtree(body.tree);

        // Serialize document tree
        Ok(Html {
            errors: Vec::new(),
            quirks_mode: QuirksMode::NoQuirks,
            tree: html,
        }
        .html())
    }
}

fn create_el(name: &str) -> Node {
    Node::Element(Element::new(create_name(name, NameKind::Element), vec![]))
}

fn create_el_with_attrs(name: &str, attrs: &[(&str, &str)]) -> Node {
    let attrs = attrs
        .iter()
        .map(|(key, value)| Attribute {
            name: create_name(key, NameKind::Attr),
            value: (*value).into(),
        })
        .collect();

    Node::Element(Element::new(create_name(name, NameKind::Element), attrs))
}

fn create_name(name: &str, kind: NameKind) -> QualName {
    QualName {
        prefix: None,
        ns: match kind {
            NameKind::Element => ns!(html),
            NameKind::Attr => ns!(),
        },
        local: LocalName::try_static(name)
            .expect("calls to this function should supply valid names"),
    }
}

#[derive(Clone, Copy)]
enum NameKind {
    Element,
    Attr,
}

#[cfg(test)]
mod test {
    use super::{create_el, create_el_with_attrs};
    use scraper::{Html, Node};

    /// Utility function for asserting that the HTML representation of `element` is equal to `expected`
    fn assert_eq_serialized(element: Node, expected: &str) {
        let mut html = Html::new_fragment();
        html.tree.root_mut().append(element);
        assert_eq!(html.html(), expected);
    }

    #[test]
    fn create_element() {
        // Non-void element
        assert_eq_serialized(create_el("p"), "<p></p>");

        // Void element
        assert_eq_serialized(create_el("img"), "<img>");
    }

    #[test]
    fn create_element_with_attrs() {
        // Non-void element with single attribute
        assert_eq_serialized(
            create_el_with_attrs("p", &[("id", "abc")]),
            "<p id=\"abc\"></p>",
        );

        // Non-void element with multiple attributes
        assert_eq_serialized(
            create_el_with_attrs("p", &[("id", "abc"), ("class", "def")]),
            "<p id=\"abc\" class=\"def\"></p>",
        );

        // Void element with single attribute
        assert_eq_serialized(
            create_el_with_attrs("img", &[("id", "abc")]),
            "<img id=\"abc\">",
        );

        // Void element with multiple attributes
        assert_eq_serialized(
            create_el_with_attrs("img", &[("id", "abc"), ("class", "def")]),
            "<img id=\"abc\" class=\"def\">",
        );
    }

    #[test]
    fn create_element_with_empty_attrs() {
        // Element with empty attribute name and value
        assert_eq_serialized(create_el_with_attrs("p", &[("", "")]), "<p =\"\"></p>");

        // Element with empty attribute name
        assert_eq_serialized(
            create_el_with_attrs("p", &[("", "abc")]),
            "<p =\"abc\"></p>",
        );

        // Element with empty attribute value
        assert_eq_serialized(create_el_with_attrs("p", &[("id", "")]), "<p id=\"\"></p>");
    }

    #[test]
    #[should_panic]
    fn create_nonexistent_element() {
        // "_" should be an invalid element name
        create_el("_");
    }

    #[test]
    #[should_panic]
    fn create_element_with_nonexistent_attrs() {
        // "_" should be an invalid attribute name
        create_el_with_attrs("p", &[("_", "abc")]);
    }
}
