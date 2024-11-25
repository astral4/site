//! Code for building complete HTML pages from article bodies.

use crate::OUTPUT_SITE_CSS_FILE;
use anyhow::{Error, Result};
use ego_tree::{tree, Tree};
use markup5ever::{namespace_url, ns, Attribute, LocalName, QualName};
use scraper::{
    node::{Doctype, Element, Node, Text},
    Html,
};

pub struct PageBuilder {
    body: Tree<Node>,
}

impl PageBuilder {
    /// Initializes the webpage HTML builder, parsing an input string as a HTML `<body>`.
    ///
    /// # Errors
    /// This function returns an error if the input string could not be successfully parsed as no-quirks HTML.
    pub fn new(body: &str) -> Result<Self> {
        let body = Html::parse_fragment(body);

        // `Html::parse_fragment()` doesn't return a `Result` because
        // the parser is supposed to be resilient and fall back to HTML quirks mode upon encountering errors.
        // So, after parsing, we have to check for any errors encountered ourselves.
        match body.errors.first() {
            Some(err) => {
                Err(Error::msg(err.clone()).context("failed to parse input as valid HTML"))
            }
            None => Ok(Self { body: body.tree }),
        }
    }

    /// Consumes the webpage HTML builder, outputting a string containing a complete HTML document.
    /// The parameters determine the contents of various metadata tags in the HTML `<head>` element.
    #[must_use]
    pub fn build_page(self, title: &str, author: &str) -> String {
        let mut html = Html::new_document();
        let mut root_node = html.tree.root_mut();

        // Add `<!DOCTYPE html>`
        root_node.append(Node::Doctype(Doctype {
            name: "html".into(),
            public_id: "".into(),
            system_id: "".into(),
        }));

        // Add `<html lang="en">`
        let mut html_el_node = root_node.append(create_el_with_attrs("html", [("lang", "en")]));

        // Add `<head>` within `<html>`
        html_el_node.append_subtree(tree! {
            create_el("head") => {
                create_el_with_attrs("meta", [("charset", "utf-8")]),
                create_el_with_attrs("meta", [("name", "viewport"), ("content", "width=device-width, initial-scale=1")]),
                create_el_with_attrs("meta", [("name", "author"), ("content", author)]),
                create_el("title") => { Node::Text(Text { text: title.into() }) },
                create_el_with_attrs("link", [("rel", "stylesheet"), ("href", OUTPUT_SITE_CSS_FILE)])
            }
        });

        // Add `<body>` within `<html>`
        let mut body_el_node = html_el_node.append(create_el("body"));
        body_el_node.append_subtree(self.body);

        // Serialize document tree
        html.html()
    }
}

fn create_el(name: &str) -> Node {
    Node::Element(Element::new(create_name(name, NameKind::Element), vec![]))
}

fn create_el_with_attrs<const N: usize>(name: &str, attrs: [(&str, &str); N]) -> Node {
    let attrs = attrs
        .map(|(key, value)| Attribute {
            name: create_name(key, NameKind::Attr),
            value: value.into(),
        })
        .to_vec();

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
    use scraper::Html;

    #[test]
    fn create_element() {
        // test serializing non-void element
        let mut html = Html::new_fragment();
        html.tree.root_mut().append(create_el("p"));
        assert_eq!(html.html(), "<p></p>");

        // test serializing void element
        let mut html = Html::new_fragment();
        html.tree.root_mut().append(create_el("img"));
        assert_eq!(html.html(), "<img>");
    }

    #[test]
    fn create_element_with_attrs() {
        // test serializing element with empty attribute name
        let mut html = Html::new_fragment();
        html.tree
            .root_mut()
            .append(create_el_with_attrs("p", [("", "")]));
        assert_eq!(html.html(), "<p =\"\"></p>");

        // test serializing element with empty attribute value
        let mut html = Html::new_fragment();
        html.tree
            .root_mut()
            .append(create_el_with_attrs("p", [("id", "")]));
        assert_eq!(html.html(), "<p id=\"\"></p>");

        // test serializing non-void element
        let mut html = Html::new_fragment();
        html.tree
            .root_mut()
            .append(create_el_with_attrs("p", [("id", "abc")]));
        assert_eq!(html.html(), "<p id=\"abc\"></p>");

        // test serializing void element
        let mut html = Html::new_fragment();
        html.tree
            .root_mut()
            .append(create_el_with_attrs("img", [("id", "abc")]));
        assert_eq!(html.html(), "<img id=\"abc\">");
    }

    #[should_panic]
    #[test]
    fn create_nonexistent_element() {
        // "_" should be an invalid element name
        create_el("_");
    }

    #[should_panic]
    #[test]
    fn create_element_with_nonexistent_attrs() {
        // "_" should be an invalid attribute name
        create_el_with_attrs("p", [("_", "abc")]);
    }
}
