//! Code for building complete HTML pages from article bodies.

use crate::{css::Font, OUTPUT_SITE_CSS_FILE};
use anyhow::{anyhow, Error, Result};
use ego_tree::{tree, NodeId, NodeMut, Tree};
use markup5ever::{interface::QuirksMode, namespace_url, ns, Attribute, LocalName, QualName};
use scraper::{
    node::{Doctype, Element, Node, Text},
    Html,
};

const OUTPUT_KATEX_CSS_FILE: &str = "/stylesheets/katex.css";

pub struct PageBuilder {
    html: Tree<Node>,
    head_id: NodeId,
    slot_id: NodeId,
    // [(selector, font-family); N]
    // font-family (key) to path (value) hashmap
    // query hashmap to get <link preload=...> attrs
}

impl PageBuilder {
    /// Initializes a webpage HTML builder. Every page built:
    /// - includes the provided author as a metadata tag
    /// - specifies preloaded fonts based on the provided list of font sources
    /// - has a `<body>` structure based on the provided template
    ///
    /// # Errors
    /// This function returns an error if:
    /// - the input template cannot be successfully parsed as no-quirks HTML
    /// - the input template does not contain a `<main>` element for slotting page content
    pub fn new(author: &str, site_fonts: &[Font], template: &str) -> Result<Self> {
        // Parse template into tree of HTML nodes
        let template = Html::parse_fragment(template);

        // `Html::parse_fragment()` doesn't return a `Result` because
        // the parser is supposed to be resilient and fall back to HTML quirks mode upon encountering errors.
        // So, after parsing, we have to check for any errors encountered ourselves.
        if let Some(err) = template.errors.first() {
            return Err(Error::msg(err.clone()).context("failed to parse input as valid HTML"));
        }

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

        // Add `<body>` within `<html>`
        let mut body_el_node = html_el_node.append(create_el("body"));

        // Add template within `<body>`
        append_fragment(&mut body_el_node, template);

        // Find element in template for slotting page content
        // We search in reverse insertion order because the template's HTML nodes were inserted last.
        let Some(slot_id) = html.tree.nodes().rev().find_map(|node| {
            node.value()
                .as_element()
                .is_some_and(|el| el.name() == "main") // "We have components at home"
                .then(|| node.id())
        }) else {
            return Err(anyhow!(
                "template does not have a `<main>` element for slotting page content"
            ));
        };

        Ok(Self {
            html: html.tree,
            head_id,
            slot_id,
        })
    }

    /// Outputs a string containing a complete HTML document based on the provided document title and body.
    ///
    /// # Errors
    /// This function returns an error if the input body cannot be successfully parsed as no-quirks HTML.
    pub fn build_page(&self, title: &str, body: &str, contains_math: bool) -> Result<String> {
        // Parse body into tree of HTML nodes
        let body = Html::parse_fragment(body);

        // `Html::parse_fragment()` doesn't return a `Result` because
        // the parser is supposed to be resilient and fall back to HTML quirks mode upon encountering errors.
        // So, after parsing, we have to check for any errors encountered ourselves.
        if let Some(err) = body.errors.first() {
            return Err(Error::msg(err.clone()).context("failed to parse input as valid HTML"));
        }

        let mut html = self.html.clone();

        // Add `<title>` within `<head>`
        // SAFETY: The ID is valid because it was generated in the constructor `PageBuilder::new()`.
        let mut head_node = unsafe { html.get_unchecked_mut(self.head_id) };

        if contains_math {
            head_node.append(create_el_with_attrs(
                "link",
                &[("rel", "stylesheet"), ("href", OUTPUT_KATEX_CSS_FILE)],
            ));
        }

        head_node.append_subtree(tree! {
            create_el("title") => { Node::Text(Text { text: title.into() }) }
        });

        // Add page content within template slot
        // SAFETY: The ID is valid because it was generated in the constructor `PageBuilder::new()`.
        let mut slot_node = unsafe { html.get_unchecked_mut(self.slot_id) };
        append_fragment(&mut slot_node, body);

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

/// Appends the contents of `fragment` as children of the input `node`.
fn append_fragment(node: &mut NodeMut<'_, Node>, fragment: Html) {
    // Fragments have the following structure:
    // Node::Fragment -> { Node::Element("html") -> { <contents> }}
    // After appending the fragment's tree, we have to make the contents direct children of the node.
    let mut fragment_root_node = node.append_subtree(fragment.tree);
    let fragment_root_id = fragment_root_node.id();
    let fragment_html_id = fragment_root_node.first_child().unwrap().id();
    node.reparent_from_id_append(fragment_html_id);
    // SAFETY: Indexing is guaranteed to be valid because
    // the ID was obtained from appending the fragment as a subtree of a node from the tree.
    unsafe { node.tree().get_unchecked_mut(fragment_root_id) }.detach();
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
