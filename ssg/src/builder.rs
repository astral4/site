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
            Some(err) => Err(Error::msg(err.clone())
                .context("encountered errors when parsing page body as HTML")),
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
    Node::Element(Element::new(create_name(name), vec![]))
}

fn create_el_with_attrs<const N: usize>(name: &str, attrs: [(&str, &str); N]) -> Node {
    let attrs = attrs
        .map(|(key, value)| Attribute {
            name: create_name(key),
            value: value.into(),
        })
        .to_vec();

    Node::Element(Element::new(create_name(name), attrs))
}

fn create_name(name: &str) -> QualName {
    QualName {
        prefix: None,
        ns: ns!(html),
        local: LocalName::try_static(name)
            .expect("calls to this function should supply valid names"),
    }
}
